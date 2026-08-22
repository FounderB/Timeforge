use std::path::Path;
use std::sync::Mutex;

use serde_json::json;
use tiny_http::{Header, Method, Response, Server};

use crate::{
    blame_map, blast_radius, bug_hunt_ex, commit_churn, contributors, dig_pattern, file_blame,
    file_hotspots, file_timeline, fix_break_pairs, ghost_authors, list_cached, list_tree_ex,
    open_github, ownership_heatmap, pr_travel, remote, repair_cache, repair_current, stale_files,
    update_repo, why_broke, Repo,
};

const INDEX: &str = include_str!("../../web/index.html");
const ICON_PNG: &[u8] = include_bytes!("../../web/icon.png");
const FAVICON_PNG: &[u8] = include_bytes!("../../web/favicon.png");

pub fn serve(addr: &str, repo_path: &Path) -> Result<(), String> {
    let initial = Repo::discover(repo_path)?;
    let state = Mutex::new(initial);
    let server = Server::http(addr).map_err(|e| e.to_string())?;
    eprintln!("Timeforge UI → http://{addr}");
    eprintln!("Repo: {}", state.lock().unwrap().path().display());
    eprintln!("Open remotes via UI or: timeforge open owner/repo");

    for mut request in server.incoming_requests() {
        let url = request.url().to_string();
        let method = request.method().clone();

        let respond = |status: u16, body: &str, ctype: &str| {
            Response::from_string(body)
                .with_status_code(status)
                .with_header(Header::from_bytes("Content-Type", ctype).unwrap())
                .with_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap())
        };

        let respond_bytes = |status: u16, body: &'static [u8], ctype: &str| {
            Response::from_data(body)
                .with_status_code(status)
                .with_header(Header::from_bytes("Content-Type", ctype).unwrap())
                .with_header(Header::from_bytes("Cache-Control", "public, max-age=86400").unwrap())
                .with_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap())
        };

        if method == Method::Options {
            let _ = request.respond(
                Response::from_string("")
                    .with_status_code(204)
                    .with_header(Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap())
                    .with_header(
                        Header::from_bytes("Access-Control-Allow-Methods", "GET,POST,OPTIONS")
                            .unwrap(),
                    )
                    .with_header(
                        Header::from_bytes("Access-Control-Allow-Headers", "Content-Type").unwrap(),
                    ),
            );
            continue;
        }

        if url == "/" || url.starts_with("/index") {
            let _ = request.respond(respond(200, INDEX, "text/html; charset=utf-8"));
            continue;
        }

        if url.starts_with("/icon.png") || url.starts_with("/avatar.png") {
            let _ = request.respond(respond_bytes(200, ICON_PNG, "image/png"));
            continue;
        }
        if url.starts_with("/favicon") {
            let _ = request.respond(respond_bytes(200, FAVICON_PNG, "image/png"));
            continue;
        }

        if url == "/api/info" {
            let repo = state.lock().unwrap();
            let partial = crate::git::is_partial_clone(repo.path());
            let net = crate::git::probe_network(repo.path(), 2500);
            let body = json!({
                "repo": repo.path().display().to_string(),
                "product": "Timeforge",
                "version": env!("CARGO_PKG_VERSION"),
                "partial": partial,
                "network": net,
            })
            .to_string();
            let _ = request.respond(respond(200, &body, "application/json"));
            continue;
        }

        if url == "/api/repos" {
            match list_cached() {
                Ok(list) => {
                    let body = serde_json::to_string_pretty(&list).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url == "/api/open" && method == Method::Post {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            let parsed = serde_json::from_str::<serde_json::Value>(&body).ok();
            let spec = parsed
                .as_ref()
                .and_then(|v| v.get("spec").and_then(|s| s.as_str()).map(|s| s.to_string()))
                .or_else(|| query_param(&format!("?{body}"), "spec"));
            let repair = parsed
                .as_ref()
                .and_then(|v| v.get("repair").and_then(|b| b.as_bool()))
                .unwrap_or(false);
            let Some(spec) = spec else {
                let _ = request.respond(respond(
                    400,
                    r#"{"error":"provide {\"spec\":\"owner/repo\"}"}"#,
                    "application/json",
                ));
                continue;
            };
            let opened = remote::parse_github_spec(&spec).and_then(|(o, n)| {
                if repair {
                    repair_cache(&o, &n)
                } else {
                    open_github(&o, &n, false, false)
                }
            });
            match opened {
                Ok(repo) => {
                    let path = repo.path().display().to_string();
                    *state.lock().unwrap() = repo;
                    let _ = request.respond(respond(
                        200,
                        &json!({"ok": true, "repo": path}).to_string(),
                        "application/json",
                    ));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url == "/api/update" && method == Method::Post {
            let repo = state.lock().unwrap();
            match update_repo(&repo) {
                Ok(u) => {
                    let body = serde_json::to_string_pretty(&u).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url == "/api/repair" && method == Method::Post {
            let path = state.lock().unwrap().path().to_path_buf();
            match repair_current(&Repo {
                root: path.clone(),
            }) {
                Ok(u) => {
                    if let Ok(repo) = Repo::discover(&path) {
                        *state.lock().unwrap() = repo;
                    }
                    let body = serde_json::to_string_pretty(&u).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        let path_q = query_param(&url, "path");
        let q = query_param(&url, "q");

        if url.starts_with("/api/tree") {
            let dir = path_q.clone().unwrap_or_default();
            let with_churn = query_param(&url, "churn")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            let repo = state.lock().unwrap();
            match list_tree_ex(&repo, &dir, with_churn) {
                Ok(t) => {
                    let body = serde_json::to_string_pretty(&t).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/map") {
            let path = path_q.clone().unwrap_or_else(|| "README.md".into());
            let repo = state.lock().unwrap();
            match blame_map(&repo, &path) {
                Ok(m) => {
                    let body = serde_json::to_string_pretty(&m).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/pr") {
            let prq = q.clone().or_else(|| query_param(&url, "pr")).unwrap_or_default();
            let repo = state.lock().unwrap();
            match pr_travel(&repo, &prq, 12) {
                Ok(p) => {
                    let body = serde_json::to_string_pretty(&p).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/ghosts") {
            let repo = state.lock().unwrap();
            match ghost_authors(&repo, 180, 20) {
                Ok(g) => {
                    let body = serde_json::to_string_pretty(&g).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/dig") {
            let pat = q.clone().unwrap_or_default();
            let repo = state.lock().unwrap();
            match dig_pattern(&repo, &pat, 20) {
                Ok(d) => {
                    let body = serde_json::to_string_pretty(&d).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/pairs") {
            let repo = state.lock().unwrap();
            match fix_break_pairs(&repo, "365 days ago", 12) {
                Ok(p) => {
                    let body = serde_json::to_string_pretty(&p).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/hunt")
            || url.starts_with("/api/radar")
            || url.starts_with("/api/ask")
        {
            let qq = q.clone().unwrap_or_default();
            let path = path_q.clone();
            let fast = query_param(&url, "fast")
                .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
                .unwrap_or(true);
            let repo = state.lock().unwrap();
            match bug_hunt_ex(&repo, &qq, path.as_deref(), "180 days ago", fast) {
                Ok(h) => {
                    let body = serde_json::to_string_pretty(&h).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/timeline") {
            let path = path_q.unwrap_or_else(|| "README.md".into());
            let repo = state.lock().unwrap();
            match file_timeline(&repo, &path, 40) {
                Ok(t) => {
                    let body = serde_json::to_string_pretty(&t).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/why") {
            let repo = state.lock().unwrap();
            match why_broke(&repo, path_q.as_deref(), "90 days ago", q.as_deref(), 8) {
                Ok(w) => {
                    let body = serde_json::to_string_pretty(&w).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/heatmap") {
            let repo = state.lock().unwrap();
            match ownership_heatmap(&repo, "180 days ago", None) {
                Ok(h) => {
                    let body = serde_json::to_string_pretty(&h).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/blast") {
            let path = path_q.unwrap_or_else(|| "src/main.rs".into());
            let repo = state.lock().unwrap();
            match blast_radius(&repo, &path, 15) {
                Ok(b) => {
                    let body = serde_json::to_string_pretty(&b).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/blame") {
            let path = path_q.unwrap_or_else(|| "README.md".into());
            let repo = state.lock().unwrap();
            match file_blame(&repo, &path, 40) {
                Ok(b) => {
                    let body = serde_json::to_string_pretty(&b).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/hotspots") {
            let repo = state.lock().unwrap();
            match file_hotspots(&repo, "180 days ago", 20) {
                Ok(h) => {
                    let body = serde_json::to_string_pretty(&h).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/stale") {
            let repo = state.lock().unwrap();
            match stale_files(&repo, 180, 30) {
                Ok(s) => {
                    let body = serde_json::to_string_pretty(&s).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/contributors") {
            let repo = state.lock().unwrap();
            match contributors(&repo, "365 days ago", 20) {
                Ok(c) => {
                    let body = serde_json::to_string_pretty(&c).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
                }
            }
            continue;
        }

        if url.starts_with("/api/churn") {
            let repo = state.lock().unwrap();
            match commit_churn(&repo, "365 days ago") {
                Ok(c) => {
                    let body = serde_json::to_string_pretty(&c).unwrap_or_default();
                    let _ = request.respond(respond(200, &body, "application/json"));
                }
                Err(e) => {
                    let _ = request.respond(respond(
                        400,
                        &json!({"error": e}).to_string(),
                        "application/json",
                    ));
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
