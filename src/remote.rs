use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git;
use crate::Repo;

/// Resolve a local path OR remote GitHub spec into a Repo.
pub fn resolve_repo(spec: &str) -> Result<Repo, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Repo::discover(&std::env::current_dir().map_err(|e| e.to_string())?);
    }

    let p = PathBuf::from(spec);
    if p.exists() {
        return Repo::discover(&p);
    }

    let (owner, name) = parse_github_spec(spec)?;
    open_github(&owner, &name, false, false)
}

pub fn open_github(
    owner: &str,
    name: &str,
    force_update: bool,
    full_clone: bool,
) -> Result<Repo, String> {
    let cache = cache_dir()?.join(format!("{owner}_{name}"));
    if cache.join(".git").exists() {
        git::harden_local_clone(&cache);
        if force_update {
            eprintln!("Timeforge: fetching updates for {owner}/{name}…");
            match git::git_in(&cache, &["fetch", "--all", "--prune"]) {
                Ok(_) => {
                    let _ = git::git_in(&cache, &["pull", "--ff-only"]);
                }
                Err(e) => eprintln!("Timeforge: fetch skipped ({e}) — using cached copy"),
            }
        }
        return Repo::discover(&cache);
    }

    fs::create_dir_all(cache_dir()?).map_err(|e| e.to_string())?;
    let url = format!("https://github.com/{owner}/{name}.git");
    eprintln!("Timeforge: cloning {url}");
    eprintln!("  → {}", cache.display());

    // Full clone by default for reliable offline timeline/blame.
    // Partial blobless clone is opt-in and can break without network (promisor).
    let mut args = vec!["clone".to_string(), "--single-branch".into()];
    if !full_clone {
        // Still avoid blob:none — use treeless only if we ever need bandwidth.
        // Prefer complete objects for the default branch.
    }
    args.push(url.clone());
    args.push(cache.display().to_string());

    let status = Command::new("git")
        .args(&args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .map_err(|e| format!("git clone failed: {e}"))?;
    if !status.success() {
        let _ = fs::remove_dir_all(&cache);
        return Err(format!(
            "failed to clone {url} — check network / repo visibility"
        ));
    }
    git::harden_local_clone(&cache);
    Repo::discover(&cache)
}

pub fn parse_github_spec(spec: &str) -> Result<(String, String), String> {
    let s = spec
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git");
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .or_else(|| s.strip_prefix("ssh://git@"))
        .unwrap_or(s);
    let s = s.strip_prefix("git@github.com:").unwrap_or(s);
    let s = s.strip_prefix("github.com/").unwrap_or(s);

    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 2 {
        return Ok((parts[0].to_string(), parts[1].to_string()));
    }
    Err(format!(
        "expected GitHub repo like owner/name or https://github.com/owner/name — got `{spec}`"
    ))
}

pub fn cache_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| "HOME not set".to_string())?;
    Ok(PathBuf::from(home).join(".timeforge").join("repos"))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CachedRepo {
    pub id: String,
    pub path: String,
    pub remote: Option<String>,
}

pub fn list_cached() -> Result<Vec<CachedRepo>, String> {
    let dir = cache_dir()?;
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.join(".git").exists() {
            continue;
        }
        let remote = git::git_in(&path, &["remote", "get-url", "origin"]).ok();
        out.push(CachedRepo {
            id: entry.file_name().to_string_lossy().into_owned(),
            path: path.display().to_string(),
            remote: remote.map(|s| s.trim().to_string()),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

pub fn is_remote_spec(spec: &str) -> bool {
    let s = spec.trim();
    if Path::new(s).exists() {
        return false;
    }
    parse_github_spec(s).is_ok()
}

/// Re-clone without partial filter when an existing cache is broken.
pub fn repair_cache(owner: &str, name: &str) -> Result<Repo, String> {
    let cache = cache_dir()?.join(format!("{owner}_{name}"));
    if cache.exists() {
        eprintln!("Timeforge: repairing cache (removing partial clone)…");
        let _ = fs::remove_dir_all(&cache);
    }
    open_github(owner, name, false, true)
}
