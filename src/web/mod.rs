use serde_json::json;
use tiny_http::{Header, Method, Response, Server};

use crate::{blast_radius, file_blame, file_timeline, ownership_heatmap, why_broke, Repo};

const INDEX: &str = include_str!("../../web/index.html");

pub fn serve(addr: &str, repo_path: &std::path::Path) -> Result<(), String> {
    let repo = Repo::discover(repo_path)?;
    let server = Server::http(addr).map_err(|e| e.to_string())?;
    eprintln!("Timeforge UI → http://{addr}");
    eprintln!("Repo: {}", repo.path().display());

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let method = request.method().clone();

        let respond = |status: u16, body: &str, ctype: &str| {
            Response::from_string(body)
                .with_status_code(status)
                .with_header(Header::from_bytes("Content-Type", ctype).unwrap())
                .with_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap())
        };

        if method == Method::Options {
            let _ = request.respond(
                Response::from_string("")
                    .with_status_code(204)
                    .with_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap())
                    .with_header(Header::from_bytes("Access-Control-Allow-Methods", "GET,POST,OPTIONS").unwrap())
                    .with_header(Header::from_bytes("Access-Control-Allow-Headers", "Content-Type").unwrap()),
            );
            continue;
        }

        if url == "/" || url.starts_with("/index") {
            let _ = request.respond(respond(200, INDEX, "text/html; charset=utf-8"));
            continue;
        }

        if url == "/api/info" {
            let body = json!({
                "repo": repo.path().display().to_string(),
                "product": "Timeforge",
                "version": env!("CARGO_PKG_VERSION"),
            })
            .to_string();
            let _ = request.respond(respond(200, &body, "application/json"));
            continue;
        }

        if url.starts_with("/api/timeline") && method == Method::Get {
            let path = query_param(&url, "path").unwrap_or_else(|| "README.md".into());
            match file_timeline(&repo, &path, 40) {
                Ok(t) => {
                    let body = serde_json::to_string_pretty(&t).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(400, &json!({"error": e}).to_string(), "application/json"));
                }
            }
            continue;
        }

        if url.starts_with("/api/why") {
            let path = query_param(&url, "path");
            let kw = query_param(&url, "q");
            match why_broke(&repo, path.as_deref(), "90 days ago", kw.as_deref(), 8) {
                Ok(w) => {
                    let body = serde_json::to_string_pretty(&w).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(400, &json!({"error": e}).to_string(), "application/json"));
                }
            }
            continue;
        }

        if url.starts_with("/api/heatmap") {
            match ownership_heatmap(&repo, "180 days ago", None) {
                Ok(h) => {
                    let body = serde_json::to_string_pretty(&h).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(400, &json!({"error": e}).to_string(), "application/json"));
                }
            }
            continue;
        }

        if url.starts_with("/api/blast") {
            let path = query_param(&url, "path").unwrap_or_else(|| "src/main.rs".into());
            match blast_radius(&repo, &path, 15) {
                Ok(b) => {
                    let body = serde_json::to_string_pretty(&b).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(400, &json!({"error": e}).to_string(), "application/json"));
                }
            }
            continue;
        }

        if url.starts_with("/api/blame") {
            let path = query_param(&url, "path").unwrap_or_else(|| "README.md".into());
            match file_blame(&repo, &path, 40) {
                Ok(b) => {
                    let body = serde_json::to_string_pretty(&b).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(400, &json!({"error": e}).to_string(), "application/json"));
                }
            }
            continue;
        }

        let _ = request.respond(respond(404, "not found", "text/plain"));
    }
    Ok(())
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let q = url.split('?').nth(1)?;
    for part in q.split('&') {
        let mut kv = part.splitn(2, '=');
        if kv.next()? == key {
            let v = kv.next().unwrap_or("");
            return Some(percent_decode(v));
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(chunk) = std::str::from_utf8(&b[i + 1..i + 3]) {
                if let Ok(v) = u8::from_str_radix(chunk, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
        }
        if b[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(b[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
