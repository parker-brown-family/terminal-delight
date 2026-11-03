//! `terminal-delight wire-replay <capture>`: does this build read a real
//! session host's bytes into the screen that host reported?
//!
//! The updater counts two builds as sharing a wire only when their source
//! fingerprints match (`build.rs`), so any change to the terminal core stages
//! an update for a cold start, even one measured to read old hosts correctly.
//! This verb is the measurement, made runnable against a shipped binary: CI
//! captures a host of the build it is replacing (`scripts/probe-capture.py`),
//! replays the capture here, and writes the wire it passed against into the
//! new build's `build.json` (`scripts/wire-attest.sh`, issue 1267).
//!
//! The comparison is the one `gridwire`'s ignored real-host test makes: feed
//! the captured bytes to a fresh core of the capture's size and compare
//! `grid_hash` with the number the host computed for its own grid.
use crate::gridwire::grid_hash;
use crate::term::GridSize;
use crate::vt::{Event, Listener, Term};
use std::sync::Arc;

const USAGE: &str = "usage: terminal-delight wire-replay <capture-prefix>\n\n\
    Reads <prefix>.bin and <prefix>.json from scripts/probe-capture.py and prints one \
    line of JSON. Exit 0: this build reads the capture into the host's screen. Exit 1: \
    it does not. Exit 2: the capture cannot be judged.";

struct Deaf;
impl Listener for Deaf {
    fn send_event(&self, _: Event) {}
}

/// What one capture says about the host that made it.
#[derive(Debug, PartialEq)]
struct Capture {
    bytes: Vec<u8>,
    cols: usize,
    rows: usize,
    host_hash: u64,
}

fn load(prefix: &str) -> Result<Capture, String> {
    let bytes = std::fs::read(format!("{prefix}.bin")).map_err(|e| format!("{prefix}.bin: {e}"))?;
    let meta = std::fs::read_to_string(format!("{prefix}.json"))
        .map_err(|e| format!("{prefix}.json: {e}"))?;
    let meta: serde_json::Value =
        serde_json::from_str(&meta).map_err(|e| format!("{prefix}.json: {e}"))?;
    let n = |k: &str| meta[k].as_u64().ok_or(format!("{prefix}.json has no {k}"));
    // A capture whose bytes stop short of the host's stream offset was taken
    // while the pane was still printing: its hash describes a grid these bytes
    // do not produce, and a match or a mismatch would both mean nothing.
    let (read, offset) = (n("bytes_read")?, n("stream_offset")?);
    if read != offset || read != bytes.len() as u64 {
        return Err(format!(
            "the capture is not aligned: {} bytes on disk, {read} read, host stream at {offset}",
            bytes.len()
        ));
    }
    let (cols, rows) = (n("cols")? as usize, n("rows")? as usize);
    if cols == 0 || rows == 0 || bytes.is_empty() {
        return Err("the capture is empty".into());
    }
    Ok(Capture {
        bytes,
        cols,
        rows,
        host_hash: n("hash")?,
    })
}

/// This build's `grid_hash` of the capture, replayed.
fn replay(c: &Capture) -> u64 {
    let size = GridSize {
        cols: c.cols,
        rows: c.rows,
    }
    .with_cell(8, 16);
    let mut term = Term::new(size, Arc::new(Deaf));
    term.advance(&c.bytes);
    grid_hash(&term)
}

pub(crate) fn run_cli(args: &[String]) -> i32 {
    let [prefix] = args else {
        eprintln!("{USAGE}");
        return 2;
    };
    let c = match load(prefix) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("terminal-delight wire-replay: {e}");
            return 2;
        }
    };
    let ours = replay(&c);
    let same = ours == c.host_hash;
    println!(
        "{}",
        serde_json::json!({
            "same": same,
            "ours": format!("{ours:#x}"),
            "host": format!("{:#x}", c.host_hash),
            "bytes": c.bytes.len(),
            "cols": c.cols,
            "rows": c.rows,
        })
    );
    if same {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &std::path::Path, bytes: &[u8], meta: serde_json::Value) -> String {
        let prefix = dir.join("cap").to_string_lossy().to_string();
        std::fs::write(format!("{prefix}.bin"), bytes).unwrap();
        std::fs::write(format!("{prefix}.json"), meta.to_string()).unwrap();
        prefix
    }

    fn meta(len: usize, hash: u64) -> serde_json::Value {
        serde_json::json!({"hash": hash, "stream_offset": len, "bytes_read": len, "cols": 20, "rows": 4})
    }

    /// A capture replays to its own hash; one changed letter, or a changed
    /// reported hash, does not; and a capture taken mid-print is refused
    /// rather than judged. The real proof is CI's replay of a real host, with
    /// its own mutated control (`scripts/wire-attest.sh`); this holds the verb.
    #[test]
    fn a_capture_matches_only_the_screen_it_was_taken_from() {
        let dir = crate::testsync::Scratch::new("wire-replay");
        let bytes = b"plain line one\r\n\x1b[44mtinted\x1b[0m\r\ndone";
        let hash = replay(&Capture {
            bytes: bytes.to_vec(),
            cols: 20,
            rows: 4,
            host_hash: 0,
        });
        let ok = write(&dir, bytes, meta(bytes.len(), hash));
        assert_eq!(run_cli(&[ok]), 0);

        let mut changed = bytes.to_vec();
        changed[6] = b'X';
        let bad = write(&dir, &changed, meta(bytes.len(), hash));
        assert_eq!(run_cli(&[bad]), 1, "one letter changed is another screen");

        let wrong_hash = write(&dir, bytes, meta(bytes.len(), hash ^ 1));
        assert_eq!(run_cli(&[wrong_hash]), 1);

        let mut short = meta(bytes.len(), hash);
        short["stream_offset"] = (bytes.len() + 5).into();
        let mid_print = write(&dir, bytes, short);
        assert_eq!(
            run_cli(&[mid_print]),
            2,
            "a misaligned capture is not judged"
        );

        assert_eq!(run_cli(&[]), 2);
        let missing = dir.join("nothing").to_string_lossy().to_string();
        assert_eq!(run_cli(&[missing]), 2);
    }
}
