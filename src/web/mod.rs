use std::path::Path;
use std::sync::Mutex;

use serde_json::json;
use tiny_http::{Header, Method, Request, Response, Server};

use crate::{
    blame_map, blast_radius, bug_hunt_ex, commit_churn, contributors, dig_pattern, file_blame,
    file_hotspots, file_timeline, fix_break_pairs, ghost_authors, list_cached, list_tree_ex,
    open_github, ownership_heatmap, pr_travel, remote, repair_cache, repair_current,
    stale_files, update_repo, why_broke, Repo,
};

const INDEX: &str = include_str!("../../web/index.html");
const ICON_PNG: &[u8] = include_bytes!("../../web/icon.png");
const FAVICON_PNG: &[u8] = include_bytes!("../../web/favicon.png");

#[derive(Clone, Default)]
pub struct ServeOpts {
    pub expose: bool,
    pub token: Option<String>,
}

pub fn serve(addr: &str, repo_path: &Path) -> Result<(), String> {
    serve_with_opts(addr, repo_path, ServeOpts::default())
}

pub fn serve_with_opts(addr: &str, repo_path: &Path, opts: ServeOpts) -> Result<(), String> {
    let token = opts
        .token
        .as_ref()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());

    if !is_loopback_bind(addr) && !opts.expose && token.is_none() {
        return Err(format!(
            "refusing non-loopback bind `{addr}` without `--expose` or `--token` / TIMEFORGE_TOKEN"
        ));
    }

    let initial = Repo::discover(repo_path)?;
    let state = Mutex::new(initial);
    let server = Server::http(addr).map_err(|e| e.to_string())?;
    eprintln!("Timeforge UI → http://{addr}");
    eprintln!("Repo: {}", state.lock().unwrap().path().display());
    if let Some(_) = &token {
        eprintln!("Auth: token required (?token= / Authorization: Bearer / X-Timeforge-Token)");
    } else if opts.expose {
        eprintln!("Warning: --expose without --token — UI is open to the network");
    }
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
                        Header::from_bytes("Access-Control-Allow-Methods", "GET,POST,DELETE,OPTIONS")
                            .unwrap(),
                    )
                    .with_header(
                        Header::from_bytes(
                            "Access-Control-Allow-Headers",
                            "Content-Type, Authorization, X-Timeforge-Token",
                        )
                        .unwrap(),
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

        if url.starts_with("/api/") {
            if let Some(tok) = &token {
                if !authorize(&request, &url, tok) {
                    let _ = request.respond(respond(
                        401,
                        r#"{"error":"unauthorized"}"#,
                        "application/json",
                    ));
                    continue;
                }
            }
        }

        if url == "/api/info" || url.starts_with("/api/info?") {
            let (repo_path, partial) = {
                let repo = state.lock().unwrap();
                (
                    repo.path().display().to_string(),
                    crate::git::is_partial_clone(repo.path()),
                )
            };
            // Never hold the repo lock during network I/O — it blocked the whole UI.
            let skip_net = url.contains("net=0") || url.contains("fast=1");
            let net = if skip_net {
                crate::git::NetworkProbe {
                    online: false,
                    has_remote: false,
                    detail: "skipped".into(),
                    ms: 0,
                }
            } else {
                let p = std::path::PathBuf::from(&repo_path);
                crate::git::probe_network(&p, 800)
            };
            let body = json!({
                "repo": repo_path,
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
            if method == Method::Delete || method == Method::Post {
                let mut body = String::new();
                let _ = request.as_reader().read_to_string(&mut body);
                let parsed = serde_json::from_str::<serde_json::Value>(&body).ok();
                let spec = parsed
                    .as_ref()
                    .and_then(|v| {
                        v.get("spec")
                            .or_else(|| v.get("id"))
                            .and_then(|s| s.as_str())
                            .map(|s| s.to_string())
                    })
                    .or_else(|| query_param(&url, "spec").or_else(|| query_param(&url, "id")));
                let clean_all = parsed
                    .as_ref()
                    .and_then(|v| v.get("clean").and_then(|b| b.as_bool()))
                    .unwrap_or(false)
                    || url.contains("clean=1");
                if clean_all {
                    match remote::clean_cached() {
                        Ok(list) => {
                            let body = json!({"ok": true, "removed": list}).to_string();
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
                let Some(spec) = spec else {
                    let _ = request.respond(respond(
                        400,
                        r#"{"error":"provide {\"spec\":\"owner/repo\"} or id"}"#,
                        "application/json",
                    ));
                    continue;
                };
                let active = state.lock().unwrap().path().to_path_buf();
                match remote::remove_cached_ex(&spec, Some(&active)) {
                    Ok(r) => {
                        let body = serde_json::to_string(&r).unwrap_or_default();
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
            // Git clone/fetch must not hold the UI mutex.
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            // Clone under lock, then drop — hunt runs long git work without blocking UI.
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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
            let repo = state.lock().unwrap().clone();
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

fn authorize(request: &Request, url: &str, expected: &str) -> bool {
    if query_param(url, "token").as_deref() == Some(expected) {
        return true;
    }
    for h in request.headers() {
        let name = h.field.as_str().to_ascii_lowercase();
        let value = h.value.as_str().to_string();
        if name == "x-timeforge-token" && value == expected {
            return true;
        }
        if name == "authorization" {
            if let Some(rest) = value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
            {
                if rest.trim() == expected {
                    return true;
                }
            }
        }
    }
    false
}

fn is_loopback_bind(addr: &str) -> bool {
    let host = bind_host(addr);
    host.is_empty()
        || host == "127.0.0.1"
        || host == "localhost"
        || host == "::1"
        || host == "[::1]"
}

fn bind_host(addr: &str) -> &str {
    if let Some(rest) = addr.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            return &rest[..end];
        }
    }
    // "127.0.0.1:8790" or ":8790"
    if let Some((h, port)) = addr.rsplit_once(':') {
        if port.chars().all(|c| c.is_ascii_digit()) {
            return h;
        }
    }
    addr
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
