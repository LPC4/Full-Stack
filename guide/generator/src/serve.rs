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
        Some("png" | "ico") => "image/png",
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

fn respond(
    stream: &mut TcpStream,
    method: &str,
    status: &str,
    kind: &str,
    extra_headers: &[(&str, &str)],
    body: &[u8],
) -> std::io::Result<()> {
    let extra = extra_headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n{extra}\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    if method == "HEAD" {
        Ok(())
    } else {
        stream.write_all(body)
    }
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
    let target = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    if target.is_empty() || !version.starts_with("HTTP/") {
        return respond(
            stream,
            method,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            &[],
            b"bad request\n",
        );
    }
    if method != "GET" && method != "HEAD" {
        return respond(
            stream,
            method,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            &[("Allow", "GET, HEAD")],
            b"method not allowed\n",
        );
    }

    let Some(path) = resolve(root, target) else {
        return respond(
            stream,
            method,
            "403 Forbidden",
            "text/plain; charset=utf-8",
            &[],
            b"forbidden\n",
        );
    };
    let request_path = target.split(['?', '#']).next().unwrap_or_default();
    if path.ends_with("index.html")
        && path.parent().is_some_and(Path::is_dir)
        && !request_path.ends_with('/')
        && !request_path.ends_with("index.html")
    {
        let query = target
            .split_once('?')
            .map(|(_, query)| format!("?{query}"))
            .unwrap_or_default();
        let location = format!("{request_path}/{query}");
        return respond(
            stream,
            method,
            "308 Permanent Redirect",
            "text/plain; charset=utf-8",
            &[("Location", &location)],
            b"redirecting to canonical route\n",
        );
    }
    let Ok(body) = std::fs::read(&path) else {
        return respond(
            stream,
            method,
            "404 Not Found",
            "text/plain; charset=utf-8",
            &[],
            b"not found\n",
        );
    };
    respond(stream, method, "200 OK", content_type(&path), &[], &body)
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
    use std::io::Read as _;

    fn exchange(root: &Path, request: &[u8]) -> Vec<u8> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listen");
        let address = listener.local_addr().expect("address");
        let request = request.to_owned();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).expect("connect");
            stream.write_all(&request).expect("request");
            let mut response = Vec::new();
            stream.read_to_end(&mut response).expect("response");
            response
        });
        let (mut stream, _) = listener.accept().expect("accept");
        handle(root, &mut stream).expect("handle");
        drop(stream);
        client.join().expect("client")
    }

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
        assert_eq!(content_type(Path::new("favicon.ico")), "image/png");
    }

    #[test]
    fn head_reports_get_length_without_a_body() {
        let base =
            std::env::temp_dir().join(format!("full-stack-guide-head-{}", std::process::id()));
        let page = base.join("guide");
        std::fs::create_dir_all(&page).expect("create route");
        std::fs::write(page.join("index.html"), "page").expect("write route");

        let response = exchange(&base, b"HEAD /guide/ HTTP/1.1\r\nHost: localhost\r\n\r\n");
        let response = String::from_utf8(response).expect("utf8 response");
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("Content-Length: 4\r\n"));
        assert!(response.ends_with("\r\n\r\n"));
        std::fs::remove_dir_all(base).expect("remove fixture");
    }

    #[test]
    fn unsupported_methods_advertise_the_uniform_interface() {
        let response = exchange(
            Path::new("out"),
            b"POST /guide/ HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        let response = String::from_utf8(response).expect("utf8 response");
        assert!(response.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));
        assert!(response.contains("Allow: GET, HEAD\r\n"));
    }

    #[test]
    fn directory_resources_redirect_to_their_canonical_route() {
        let base =
            std::env::temp_dir().join(format!("full-stack-guide-redirect-{}", std::process::id()));
        let page = base.join("guide");
        std::fs::create_dir_all(&page).expect("create route");
        std::fs::write(page.join("index.html"), "page").expect("write route");

        let response = exchange(
            &base,
            b"GET /guide?source=test HTTP/1.1\r\nHost: localhost\r\n\r\n",
        );
        let response = String::from_utf8(response).expect("utf8 response");
        assert!(response.starts_with("HTTP/1.1 308 Permanent Redirect\r\n"));
        assert!(response.contains("Location: /guide/?source=test\r\n"));
        std::fs::remove_dir_all(base).expect("remove fixture");
    }
}
