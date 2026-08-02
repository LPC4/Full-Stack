// A local preview server. The sandbox loads an ES module and instantiates wasm,
// both of which browsers refuse over file://, so reading the output needs http.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        // Anything else makes instantiateStreaming reject the module.
        Some("wasm") => "application/wasm",
        Some("png") => "image/png",
        Some("toml" | "txt" | "md" | "rs" | "hll" | "s" | "ld" | "yml" | "yaml") => {
            "text/plain; charset=utf-8"
        }
        _ => "application/octet-stream",
    }
}

// Keeps a crafted URL from reaching outside the output directory.
fn resolve(root: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or("");
    let mut resolved = root.to_path_buf();
    for part in path.split('/').filter(|p| !p.is_empty()) {
        let decoded = percent_decode(part);
        let relative = Path::new(&decoded);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return None;
        }
        resolved.push(relative);
    }
    if resolved.is_dir() {
        resolved.push("index.html");
    }
    Some(resolved)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)
}

fn handle(root: &Path, stream: &mut TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;

    // Drain headers so the client sees the response rather than a reset.
    let mut header = String::new();
    while reader.read_line(&mut header)? > 2 {
        header.clear();
    }

    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");
    if method != "GET" && method != "HEAD" {
        return respond(
            stream,
            "405 Method Not Allowed",
            "text/plain",
            b"only GET\n",
        );
    }

    let Some(path) = resolve(root, target) else {
        return respond(stream, "403 Forbidden", "text/plain", b"forbidden\n");
    };
    let Ok(body) = std::fs::read(&path) else {
        return respond(stream, "404 Not Found", "text/plain", b"not found\n");
    };
    respond(stream, "200 OK", content_type(&path), &body)
}

pub fn serve(root: &Path, port: u16) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("cannot listen on port {port}: {e}"))?;
    let actual = listener.local_addr().map(|a| a.port()).unwrap_or(port);
    println!(
        "serving {} at http://localhost:{actual}/guide/",
        root.display()
    );
    println!("press Ctrl+C to stop");

    for incoming in listener.incoming() {
        // One bad connection is not a reason to stop serving the rest.
        let Ok(mut stream) = incoming else { continue };
        let root = root.to_path_buf();
        std::thread::spawn(move || {
            let _result: std::io::Result<()> = handle(&root, &mut stream);
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_is_rejected() {
        let root = Path::new("out");
        assert!(resolve(root, "/../../secret").is_none());
        assert!(resolve(root, "/%2e%2e/secret").is_none());
    }

    #[test]
    fn paths_resolve_under_the_root() {
        let root = Path::new("out");
        assert_eq!(
            resolve(root, "/guide/theme/guide.css?v=2"),
            Some(root.join("guide").join("theme").join("guide.css"))
        );
    }

    #[test]
    fn query_strings_do_not_change_directory_routes() {
        let base =
            std::env::temp_dir().join(format!("full-stack-guide-serve-{}", std::process::id()));
        let page = base.join("guide/chapter/page");
        std::fs::create_dir_all(&page).expect("create route");
        std::fs::write(page.join("index.html"), "page").expect("write route");
        assert_eq!(
            resolve(&base, "/guide/chapter/page/?from=test"),
            Some(page.join("index.html"))
        );
        std::fs::remove_dir_all(base).expect("remove fixture");
    }

    #[test]
    fn wasm_gets_its_own_type() {
        assert_eq!(content_type(Path::new("a/b.wasm")), "application/wasm");
        assert_eq!(content_type(Path::new("a/b.svg")), "image/svg+xml");
        assert_eq!(content_type(Path::new("a/b.json")), "application/json");
    }
}
