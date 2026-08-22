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
            match git::git_network_in(&cache, &["fetch", "--all", "--prune"]) {
                Ok(_) => {
                    let _ = git::git_network_in(&cache, &["pull", "--ff-only"]);
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

/// Fetch + ff-pull the current repo; materialize partial clones when needed.
pub fn update_repo(repo: &Repo) -> Result<UpdateResult, String> {
    let path = repo.path();
    let before = git::git_in(path, &["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|_| "?".into())
        .trim()
        .to_string();

    let remote = git::git_network_in(path, &["remote"]).unwrap_or_default();
    if remote.trim().is_empty() {
        return Ok(UpdateResult {
            ok: true,
            before: before.clone(),
            after: before,
            message: "no remotes configured — local-only repo".into(),
            partial: false,
        });
    }

    let mut notes = Vec::new();
    let partial = git::is_partial_clone(path);
    if partial {
        match git::materialize_partial(path) {
            Ok(m) => notes.push(m),
            Err(e) => notes.push(format!("materialize soft-fail: {e}")),
        }
    }

    git::git_network_in(path, &["fetch", "--all", "--prune"])?;

    let pull = git::git_network_in(path, &["pull", "--ff-only"]);
    let after = git::git_in(path, &["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|_| before.clone())
        .trim()
        .to_string();

    let message = match pull {
        Ok(out) => {
            let t = out.trim();
            let core = if before == after {
                if t.is_empty() || t.contains("Already up to date") {
                    "already up to date".into()
                } else {
                    format!("fetched · HEAD still {after}")
                }
            } else {
                format!("updated {before} → {after}")
            };
            if notes.is_empty() {
                core
            } else {
                format!("{core} · {}", notes.join(" · "))
            }
        }
        Err(e) => {
            if before != after {
                format!("fetched; pull note: {e}")
            } else if partial {
                return Err(format!(
                    "partial clone still missing objects ({e}). Use Repair clone."
                ));
            } else {
                return Err(format!("update failed: {e}"));
            }
        }
    };

    Ok(UpdateResult {
        ok: true,
        before,
        after,
        message,
        partial: git::is_partial_clone(path),
    })
}

/// Re-clone the currently open repo from its origin (fixes broken promisor caches).
pub fn repair_current(repo: &Repo) -> Result<UpdateResult, String> {
    let path = repo.path().to_path_buf();
    let before = git::git_in(&path, &["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|_| "?".into())
        .trim()
        .to_string();

    let url = git::git_network_in(&path, &["remote", "get-url", "origin"])
        .map_err(|_| {
            "no origin remote — open with timeforge open owner/repo --repair".to_string()
        })?
        .trim()
        .to_string();

    // Fast path: materialize in place
    if let Ok(msg) = git::materialize_partial(&path) {
        // Verify a cheap history command works
        if git::git_in(&path, &["log", "-1", "--oneline"]).is_ok() {
            let after = git::git_in(&path, &["rev-parse", "--short", "HEAD"])
                .unwrap_or(before.clone())
                .trim()
                .to_string();
            return Ok(UpdateResult {
                ok: true,
                before,
                after,
                message: format!("repaired in place · {msg}"),
                partial: git::is_partial_clone(&path),
            });
        }
    }

    // Full re-clone into the same directory
    let parent = path
        .parent()
        .ok_or_else(|| "cannot determine parent of repo".to_string())?
        .to_path_buf();
    let name = path
        .file_name()
        .ok_or_else(|| "cannot determine repo folder name".to_string())?
        .to_string_lossy()
        .into_owned();
    let tmp = parent.join(format!(".tf-repair-{name}"));
    let _ = fs::remove_dir_all(&tmp);

    eprintln!("Timeforge: full re-clone of {url}");
    let status = Command::new("git")
        .args([
            "clone",
            "--single-branch",
            &url,
            &tmp.display().to_string(),
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .map_err(|e| format!("git clone failed: {e}"))?;
    if !status.success() {
        let _ = fs::remove_dir_all(&tmp);
        return Err(format!("failed to re-clone {url}"));
    }

    let backup = parent.join(format!(".tf-old-{name}"));
    let _ = fs::remove_dir_all(&backup);
    fs::rename(&path, &backup).map_err(|e| e.to_string())?;
    if let Err(e) = fs::rename(&tmp, &path) {
        let _ = fs::rename(&backup, &path);
        return Err(format!("swap failed: {e}"));
    }
    let _ = fs::remove_dir_all(&backup);
    git::harden_local_clone(&path);

    let after = git::git_in(&path, &["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|_| before.clone())
        .trim()
        .to_string();

    Ok(UpdateResult {
        ok: true,
        before,
        after: after.clone(),
        message: format!("full re-clone complete · HEAD {after}"),
        partial: false,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateResult {
    pub ok: bool,
    pub before: String,
    pub after: String,
    pub message: String,
    pub partial: bool,
}
