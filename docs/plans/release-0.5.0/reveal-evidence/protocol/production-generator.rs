fn path_to_file_uri(path: &str) -> String {
    let mut out = String::from("file://");
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn reveal_script(path: &str, use_uwsm: bool) -> String {
    let parent = std::path::Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "/".to_string());
    let opener = if use_uwsm {
        "uwsm-app -- xdg-open"
    } else {
        "xdg-open"
    };
    format!(
        "dbus-send --session --print-reply --reply-timeout={REVEAL_REPLY_TIMEOUT_MS} \
         --dest=org.freedesktop.FileManager1 \
         /org/freedesktop/FileManager1 org.freedesktop.FileManager1.ShowItems \
         array:string:{} string:'' >/dev/null 2>&1 || {opener} {} >/dev/null 2>&1",
        shell_quote(&path_to_file_uri(path)),
        shell_quote(&parent),
    )
}

const REVEAL_REPLY_TIMEOUT_MS: u32 = 3000;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    println!("{}", reveal_script(&args[1], args[2] == "uwsm"));
}
