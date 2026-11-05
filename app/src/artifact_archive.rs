//! Durable discovery over the conversation records already written by TD.
//! Captured bytes supplement those records; the archive page is rebuilt from them.
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::benchstore::{self, Rec};
use crate::handover::{self, Handover};

const CAPTURE_LIMIT: u64 = 16 * 1024 * 1024;
const RECORD_LIMIT: u64 = 64 * 1024 * 1024;

/// None means there are no measured bytes, never an empty file's hash or size.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub revision: Option<String>,
    pub bytes: Option<u64>,
    pub snapshot: Option<String>,
    pub unavailable: Option<String>,
}

impl Observation {
    fn unavailable(why: impl Into<String>) -> Self {
        Self {
            revision: None,
            bytes: None,
            snapshot: None,
            unavailable: Some(why.into()),
        }
    }
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The target's identity is independent of any revision of the file at that target.
pub fn artifact_id(href: &str) -> String {
    digest(handover::key_of(href).as_bytes())
}

fn read_regular(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let before = file.metadata()?;
    if !before.is_file() || before.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a regular file within the capture limit",
        ));
    }
    let mut bytes = Vec::new();
    (&file).take(limit + 1).read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    if bytes.len() as u64 > limit
        || before.len() != after.len()
        || before.modified()? != after.modified()?
        || after.len() != bytes.len() as u64
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file changed during capture",
        ));
    }
    Ok(bytes)
}

/// Only validated names can be resolved into the revision pool, including names
/// read from old or hand-edited records.
pub fn snapshot_path(store: &Path, name: &str) -> Option<PathBuf> {
    let (hash, ext) = name.rsplit_once('.')?;
    (benchstore::safe_hash(hash)
        && matches!(
            ext,
            "md" | "html" | "txt" | "json" | "svg" | "png" | "jpg" | "pdf" | "bin"
        ))
    .then(|| store.join("artifact-revisions").join(name))
}

fn capture(store: &Path, href: &str) -> Observation {
    let Some(path) = handover::file_path(href) else {
        return Observation::unavailable("Remote link; no local bytes captured.");
    };
    let bytes = match read_regular(&path, CAPTURE_LIMIT) {
        Ok(bytes) => bytes,
        Err(err) => return Observation::unavailable(format!("Local revision unavailable: {err}")),
    };
    let revision = digest(&bytes);
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some("md" | "markdown") => "md",
        Some("html" | "htm") => "html",
        Some("txt") => "txt",
        Some("json") => "json",
        Some("svg") => "svg",
        Some("png") => "png",
        Some("jpg" | "jpeg") => "jpg",
        Some("pdf") => "pdf",
        _ => "bin",
    };
    let name = format!("{revision}.{ext}");
    let target = snapshot_path(store, &name).expect("a hash and a fixed extension");
    let result = (|| -> io::Result<()> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(target.parent().unwrap())?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&target)
        {
            Ok(mut file) => {
                if let Err(err) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
                    let _ = fs::remove_file(&target);
                    return Err(err);
                }
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                if digest(&read_regular(&target, CAPTURE_LIMIT)?) != revision {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "saved revision failed its content hash",
                    ));
                }
            }
            Err(err) => return Err(err),
        }
        Ok(())
    })();
    Observation {
        revision: Some(revision),
        bytes: Some(bytes.len() as u64),
        snapshot: result.as_ref().ok().map(|_| name),
        unavailable: result
            .err()
            .map(|err| format!("Revision measured; snapshot could not be saved: {err}")),
    }
}

struct Capture {
    store: PathBuf,
    record_dir: PathBuf,
    root: String,
    said: String,
    said_at_ms: u64,
    href: String,
}

enum Job {
    Capture(Capture),
    Drain(mpsc::Sender<()>),
}

static CAPTURES: OnceLock<Result<mpsc::SyncSender<Job>, String>> = OnceLock::new();

/// Enqueue only newly filed artifact events. Replaying an unchanged mailbox must
/// not claim that today's bytes were the bytes delivered in an older handover.
pub fn observe(store: &Path, record_dir: &Path, root: &str, rec: &Rec) {
    let Rec::Said { surface, at_ms, .. } = rec else {
        return;
    };
    let Some(handed) = handover::from_surface(surface, *at_ms) else {
        return;
    };
    let sender = CAPTURES.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<Job>(128);
        std::thread::Builder::new()
            .name("td-artifacts".into())
            .spawn(move || {
                for job in rx {
                    let job = match job {
                        Job::Capture(job) => job,
                        Job::Drain(reply) => {
                            let _ = reply.send(());
                            continue;
                        }
                    };
                    let observation = capture(&job.store, &job.href);
                    let record = Rec::ArtifactRevision {
                        at_ms: crate::surfacefeed::now_ms(),
                        said: job.said,
                        said_at_ms: job.said_at_ms,
                        observation,
                    };
                    if let Err(err) = benchstore::append(&job.record_dir, &job.root, &record) {
                        eprintln!("td: artifact revision could not be recorded: {err}");
                    }
                }
            })
            .map(|_| tx)
            .map_err(|e| e.to_string())
    });
    match sender {
        Ok(sender) => {
            if sender
                .try_send(Job::Capture(Capture {
                    store: store.into(),
                    record_dir: record_dir.into(),
                    root: root.into(),
                    said: rec.identity(),
                    said_at_ms: *at_ms,
                    href: handed.href,
                }))
                .is_err()
            {
                eprintln!("td: artifact capture queue unavailable; revision remains unavailable");
            }
        }
        Err(err) => eprintln!("td: artifact capture unavailable: {err}"),
    }
}

fn drain() -> bool {
    let Some(Ok(sender)) = CAPTURES.get() else {
        return true;
    };
    let (tx, rx) = mpsc::channel();
    sender.try_send(Job::Drain(tx)).is_ok()
        && rx
            // A test shares the machine with every other test and whatever else
            // is building: two seconds failed at a load of 35, with nothing wrong.
            .recv_timeout(std::time::Duration::from_secs(if cfg!(test) {
                30
            } else {
                2
            }))
            .is_ok()
}

/// Before a conversation is known, preserve only the artifact surface in an
/// explicitly unbound record. No invented conversation or empty prompt is filed.
pub fn file_unbound(store: &Path, pane_scope: &str, rec: &Rec) -> io::Result<()> {
    let Rec::Said { surface, at_ms, .. } = rec else {
        return Ok(());
    };
    if handover::from_surface(surface, *at_ms).is_none() {
        return Ok(());
    }
    let dir = store.join("unbound");
    let root = digest(pane_scope.as_bytes());
    let mut writer = benchstore::Writer::open(&dir, &root);
    if writer.write(&dir, rec.clone()) {
        observe(store, &dir, &root, rec);
    } else if !benchstore::records(&dir, &root)
        .iter()
        .any(|r| r.identity() == rec.identity())
    {
        return Err(io::Error::other("unbound artifact could not be persisted"));
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub handed: Handover,
    pub origin: Option<String>,
    pub id: String,
    pub observation: Option<Observation>,
    pub captured_ms: Option<u64>,
}

#[derive(Default)]
pub struct Archive {
    pub entries: Vec<Entry>,
    pub problems: Vec<String>,
}

fn read_records(path: &Path) -> io::Result<(Vec<Rec>, Vec<String>)> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("not a regular record"));
    }
    let mut bytes = Vec::new();
    file.take(RECORD_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > RECORD_LIMIT {
        return Err(io::Error::other("record exceeds archive scan limit"));
    }
    let text = std::str::from_utf8(&bytes).map_err(io::Error::other)?;
    let mut records = Vec::new();
    let mut problems = Vec::new();
    for (i, line) in text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
    {
        match serde_json::from_str(line)
            .ok()
            .as_ref()
            .and_then(Rec::from_json)
        {
            Some(record) => records.push(record),
            None => problems.push(format!(
                "{}: line {} could not be read",
                path.display(),
                i + 1
            )),
        }
    }
    Ok((records, problems))
}

pub fn load(store: &Path) -> Archive {
    let mut archive = Archive::default();
    let mut events = Vec::new();
    let mut bound_events: HashMap<(String, u64), std::collections::HashSet<String>> =
        HashMap::new();
    let mut unbound_evidence = HashMap::new();
    for (dir, bound) in [(store.to_path_buf(), true), (store.join("unbound"), false)] {
        let files = match fs::read_dir(&dir) {
            Ok(files) => files,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => {
                archive
                    .problems
                    .push(format!("Could not read {}: {err}", dir.display()));
                continue;
            }
        };
        for file in files {
            let file = match file {
                Ok(file) => file,
                Err(err) => {
                    archive.problems.push(err.to_string());
                    continue;
                }
            };
            let path = file.path();
            if path.extension().and_then(|s| s.to_str()) != Some("ws") {
                continue;
            }
            let records = match read_records(&path) {
                Ok((records, problems)) => {
                    archive.problems.extend(problems);
                    records
                }
                Err(err) => {
                    archive.problems.push(format!("{}: {err}", path.display()));
                    continue;
                }
            };
            let origin = bound.then(|| {
                file.file_name()
                    .to_string_lossy()
                    .trim_end_matches(".ws")
                    .to_string()
            });
            let observed: HashMap<_, _> = records
                .iter()
                .filter_map(|r| match r {
                    Rec::ArtifactRevision {
                        said,
                        said_at_ms,
                        at_ms,
                        observation,
                    } => Some(((said.clone(), *said_at_ms), (*at_ms, observation.clone()))),
                    _ => None,
                })
                .collect();
            for rec in &records {
                let Rec::Said { surface, at_ms, .. } = rec else {
                    continue;
                };
                let Some(handed) = handover::from_surface(surface, *at_ms) else {
                    continue;
                };
                let key = (rec.identity(), *at_ms);
                let evidence = observed.get(&key).cloned();
                if let Some(origin) = &origin {
                    bound_events
                        .entry(key.clone())
                        .or_default()
                        .insert(origin.clone());
                } else if let Some(evidence) = &evidence {
                    unbound_evidence.insert(key.clone(), evidence.clone());
                }
                events.push((key, origin.clone(), handed, evidence));
            }
        }
    }
    let mut entries = BTreeMap::new();
    for (event, origin, handed, mut evidence) in events {
        // Binding moves the same held Said record, including its timestamp.
        // Join only that exact event; matching a path alone cannot establish who
        // handed it over. Multiple claims leave the unbound origin unavailable.
        let uniquely_bound = bound_events
            .get(&event)
            .is_some_and(|roots| roots.len() == 1);
        if origin.is_none() && uniquely_bound {
            continue;
        }
        if evidence.is_none() && uniquely_bound {
            evidence = unbound_evidence.get(&event).cloned();
        }
        let observation = evidence.as_ref().map(|(_, o)| o.clone());
        let revision = observation.as_ref().and_then(|o| o.revision.clone());
        let id = artifact_id(&handed.href);
        let key = (origin.clone(), id.clone(), revision);
        let entry = Entry {
            handed,
            origin,
            id,
            observation,
            captured_ms: evidence.map(|(at, _)| at),
        };
        match entries.get(&key) {
            Some(old) if old_timestamp(old) >= entry.handed.at_ms => {}
            _ => {
                entries.insert(key, entry);
            }
        }
    }
    archive.entries = entries.into_values().collect();
    archive
        .entries
        .sort_by(|a, b| b.handed.at_ms.cmp(&a.handed.at_ms).then(a.id.cmp(&b.id)));
    archive
}

fn old_timestamp(entry: &Entry) -> u64 {
    entry.handed.at_ms
}

fn html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn file_url(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path.to_string_lossy().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~".contains(byte) {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn date(ms: u64, l: crate::lang::Lang) -> String {
    let p = l.strings().parts;
    handover::local(ms)
        .map(|c| {
            let date = format!(
                "{:04}-{:02}-{:02} {:02}:{:02}",
                c.year, c.month, c.day, c.hour, c.minute
            );
            crate::lang::fill(p.archive_date, &[("date", &date)])
        })
        .unwrap_or_else(|| p.archive_unavailable.into())
}

// Stored observations and operating-system errors remain language-free records.
// Translate TD's own diagnostics only when a person opens the archive.
fn diagnostic(reason: &str, l: crate::lang::Lang) -> String {
    let p = l.strings().parts;
    match reason {
        "not a regular file within the capture limit" => return p.archive_file_limit.into(),
        "file changed during capture" => return p.archive_file_changed.into(),
        "Remote link; no local bytes captured." => return p.archive_remote.into(),
        "saved revision failed its content hash" => return p.archive_saved_hash.into(),
        "not a regular record" => return p.archive_not_regular.into(),
        "record exceeds archive scan limit" => return p.archive_scan_limit.into(),
        _ => {}
    }
    for (prefix, template) in [
        ("Local revision unavailable: ", p.archive_local_missing),
        (
            "Revision measured; snapshot could not be saved: ",
            p.archive_save_failed,
        ),
    ] {
        if let Some(detail) = reason.strip_prefix(prefix) {
            return crate::lang::fill(template, &[("reason", &diagnostic(detail, l))]);
        }
    }
    if let Some((path, line)) = reason.rsplit_once(": line ") {
        if let Some(n) = line.strip_suffix(" could not be read") {
            return crate::lang::fill(p.archive_line_unreadable, &[("path", path), ("n", n)]);
        }
    }
    if let Some(detail) = reason.strip_prefix("Could not read ") {
        if let Some((path, why)) = detail.rsplit_once(": ") {
            return crate::lang::fill(
                p.archive_path_unreadable,
                &[("path", path), ("reason", &diagnostic(why, l))],
            );
        }
    }
    // Path-prefixed record failures retain the exact path and OS diagnostics.
    if let Some((path, why)) = reason.rsplit_once(": ") {
        let translated = diagnostic(why, l);
        return format!("{path}: {translated}");
    }
    reason.into()
}

pub fn write_page(store: &Path) -> io::Result<PathBuf> {
    write_page_in(store, crate::lang::current())
}

fn write_page_in(store: &Path, l: crate::lang::Lang) -> io::Result<PathBuf> {
    let p = l.strings().parts;
    let completed = drain();
    let mut archive = load(store);
    if !completed {
        archive.problems.push(p.archive_pending.into());
    }
    let mut rows = String::new();
    let mut checked: HashMap<String, Result<PathBuf, String>> = HashMap::new();
    for entry in &archive.entries {
        let origin = entry
            .origin
            .as_deref()
            .unwrap_or(p.archive_conversation_unavailable);
        let observation = entry.observation.as_ref();
        let revision = observation.and_then(|o| o.revision.as_deref());
        let saved = observation.and_then(|o| o.snapshot.as_deref()).map(|name| {
            checked
                .entry(name.to_string())
                .or_insert_with(|| {
                    let path = snapshot_path(store, name)
                        .ok_or_else(|| p.archive_name_invalid.to_string())?;
                    let expected = name.split_once('.').map(|(hash, _)| hash).unwrap_or("");
                    let bytes = read_regular(&path, CAPTURE_LIMIT).map_err(|e| {
                        crate::lang::fill(
                            p.archive_revision_unavailable,
                            &[("reason", &diagnostic(&e.to_string(), l))],
                        )
                    })?;
                    if digest(&bytes) != expected {
                        return Err(p.archive_hash_changed.into());
                    }
                    Ok(path)
                })
                .clone()
        });
        let snapshot_problem = saved.as_ref().and_then(|r| r.as_ref().err()).cloned();
        let saved = saved.and_then(Result::ok);
        let live = handover::file_path(&entry.handed.href);
        let current = match live.as_ref() {
            Some(path) if path.is_file() => p.archive_current_exists,
            Some(_) => p.archive_current_unavailable,
            None => p.archive_remote_unchecked,
        };
        let snapshot = saved
            .map(|path| {
                format!(
                    "<a href=\"{}\">{}</a>",
                    html(&file_url(&path)),
                    html(p.archive_open_revision)
                )
            })
            .unwrap_or_else(|| format!("<span>{}</span>", html(p.archive_copy_unavailable)));
        let reason = observation
            .and_then(|o| o.unavailable.as_deref())
            .unwrap_or(if observation.is_none() {
                p.archive_no_record
            } else {
                ""
            });
        let reason = snapshot_problem.unwrap_or_else(|| diagnostic(reason, l));
        let target = match live.as_ref() {
            Some(path) => Some(file_url(path)),
            None if entry.handed.href.starts_with("https://")
                || entry.handed.href.starts_with("http://") =>
            {
                Some(entry.handed.href.clone())
            }
            None => None,
        };
        let current_link = target
            .map(|href| {
                format!(
                    "<a href=\"{}\">{}</a>",
                    html(&href),
                    html(p.archive_open_target)
                )
            })
            .unwrap_or_else(|| html(p.archive_cannot_open));
        let search = format!("{} {} {}", entry.handed.label, entry.handed.href, origin);
        rows.push_str(&format!(
            "<article data-search=\"{}\"><h2>{}</h2><p>{current_link} · {snapshot}</p><p>{} {}</p><details><summary>{}</summary><p>{}: {}</p><p>{}: {}</p><p>{}: <code>{}</code></p><p>{}: <code>{}</code></p><p>{}: {} · {}: {}</p></details></article>",
            html(&search.to_lowercase()), html(&entry.handed.label), html(current), html(&reason),
            html(p.archive_source_revision), html(p.archive_origin), html(origin),
            html(p.archive_target), html(&entry.handed.href), html(p.archive_identity), entry.id,
            html(p.archive_hash), revision.map(html).unwrap_or_else(|| html(p.archive_unavailable)),
            html(p.archive_handed), html(&date(entry.handed.at_ms, l)), html(p.archive_captured),
            entry.captured_ms.map(|ms| html(&date(ms, l))).unwrap_or_else(|| html(p.archive_unavailable))
        ));
    }
    let problems: String = archive
        .problems
        .iter()
        .map(|p| format!("<li>{}</li>", html(&diagnostic(p, l))))
        .collect();
    let warning = if problems.is_empty() {
        String::new()
    } else {
        format!(
            "<aside>{}<ul>{problems}</ul></aside>",
            html(p.archive_incomplete)
        )
    };
    let title = html(p.archive_title);
    let intro = html(p.archive_intro);
    let find = html(p.archive_find);
    let search = html(p.archive_search);
    let count = html(&l.count(&p.archive_count, archive.entries.len()));
    // JSON plus '<' escaping prevents a translated string from ending script.
    let filtered = serde_json::to_string(p.archive_count_filtered)
        .expect("string JSON")
        .replace('<', "\\u003c");
    let language = l.code();
    let body = format!(
        r#"<!doctype html><html lang="{language}"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title}</title><style>
:root{{color-scheme:dark}}body{{margin:0 auto;padding:24px;max-width:900px;background:#111827;color:#e5e7eb;font:16px/1.55 system-ui}}h1{{font-size:28px}}h2{{font-size:19px;margin:0}}a{{color:#93c5fd}}input{{box-sizing:border-box;width:100%;padding:12px;background:#1f2937;color:inherit;border:1px solid #6b7280;border-radius:6px;font:inherit}}article{{padding:20px 0;border-bottom:1px solid #374151}}p{{margin:8px 0}}details{{font-size:13px;color:#cbd5e1;overflow-wrap:anywhere}}aside{{padding:14px;background:#512e19}}code{{overflow-wrap:anywhere}}[hidden]{{display:none}}
</style><h1>{title}</h1><p>{intro}</p><label for="search">{find}</label><input id="search" type="search" placeholder="{search}"><p id="count">{count}</p>{warning}<main>{rows}</main><script>
const rows=[...document.querySelectorAll('article')],input=document.querySelector('input'),count=document.querySelector('#count');input.addEventListener('input',()=>{{const q=input.value.toLowerCase();let n=0;for(const row of rows){{row.hidden=!row.dataset.search.includes(q);if(!row.hidden)n++;}}count.textContent={filtered}.replaceAll('{{n}}',n).replaceAll('{{total}}',rows.length);}});
</script></html>"#
    );
    let path = store.join("artifact-archive.html");
    handover::write_if_changed(&path, &body)?;
    Ok(path)
}

#[cfg(test)]
mod tests;
