//! A tiny static file server for the `web/` folder.
//!
//! Host-only development tool: it exists so `cargo run -- --serve` gives you a
//! URL to open, because a browser will not fetch `.wasm` or a worklet over
//! `file://`. It is not part of the artifact.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

pub fn run(port: u16) -> io::Result<()> {
    let root = PathBuf::from("web");
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!("serving {} at http://127.0.0.1:{port}/  (Ctrl-C to stop)", root.display());
    for stream in listener.incoming() {
        if let Ok(mut stream) = stream {
            let _ = handle(&mut stream, &root);
        }
    }
    Ok(())
}

fn handle(stream: &mut TcpStream, root: &Path) -> io::Result<()> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let head_only = request.starts_with("HEAD ");
    let target = request.split_whitespace().nth(1).unwrap_or("/");
    let target = target.split('?').next().unwrap_or("/");
    // Requests are percent-encoded, so "/wav/my%20track.wav" must be decoded
    // before it can name a file on disk.
    let decoded = String::from_utf8_lossy(&percent_decode(target)).to_string();
    let rel = if decoded == "/" { "index.html".to_string() } else { decoded.trim_start_matches('/').to_string() };

    if rel.contains("..") {
        return not_found(stream);
    }
    // A generated listing of the bundled renders, so the page can find one
    // whatever the file happens to be called.
    if rel == "wav/manifest.json" {
        return wav_manifest(stream, root);
    }
    let file = root.join(&rel);
    match std::fs::read(&file) {
        Ok(bytes) => {
            let mime = mime_for(&file);
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                bytes.len()
            );
            stream.write_all(header.as_bytes())?;
            if !head_only {
                stream.write_all(&bytes)?;
            }
        }
        Err(_) => return not_found(stream),
    }
    Ok(())
}

/// Decode `%XX` escapes in a request target.
fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hi = (b[i + 1] as char).to_digit(16);
            let lo = (b[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The `.wav` files in `root/wav`, sorted. Shared by the live manifest and by
/// the one written into the folder, so a statically hosted `web/` needs no
/// server to discover its render.
pub fn wav_names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root.join("wav")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.to_ascii_lowercase().ends_with(".wav") {
                names.push(name);
            }
        }
    }
    names.sort();
    names
}

/// Write `root/wav/manifest.json`, listing whatever renders are bundled. Called
/// whenever an artifact is emitted, so hosting the `web/` folder as-is works.
pub fn write_wav_manifest(root: &Path) -> io::Result<()> {
    let dir = root.join("wav");
    if !dir.is_dir() {
        return Ok(());
    }
    let names = wav_names(root);
    let list = names.iter().map(|n| json_string(n)).collect::<Vec<_>>().join(",");
    std::fs::write(dir.join("manifest.json"), format!("[{list}]"))
}

/// `[ "name.wav", … ]` for everything in `web/wav/`, sorted.
fn wav_manifest(stream: &mut TcpStream, root: &Path) -> io::Result<()> {
    let names = wav_names(root);
    let list = names.iter().map(|n| json_string(n)).collect::<Vec<_>>().join(",");
    let body = format!("[{list}]");
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())
}

fn not_found(stream: &mut TcpStream) -> io::Result<()> {
    let body = b"404 not found";
    let header = format!(
        "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("wav") => "audio/wav",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("css") => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    }
}
