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
    _full_clone: bool,
) -> Result<Repo, String> {
    let cache = cache_dir()?.join(format!("{owner}_{name}"));
    if cache.join(".git").exists() {
        git::harden_local_clone(&cache);
        if force_update {
            eprintln!("Timeforge: fetching updates for {owner}/{name} (in place)…");
            let repo = Repo::discover(&cache)?;
            match sync_inplace(&repo, false) {
                Ok(u) => eprintln!("Timeforge: {}", u.message),
                Err(e) => eprintln!("Timeforge: fetch skipped ({e}) — using cached copy"),
            }
        }
        return Repo::discover(&cache);
    }

    fs::create_dir_all(cache_dir()?).map_err(|e| e.to_string())?;
    let url = format!("https://github.com/{owner}/{name}.git");
    eprintln!("Timeforge: cloning {url}");
    eprintln!("  → {}", cache.display());

    let status = Command::new("git")
        .args([
            "clone",
            "--single-branch",
            &url,
            &cache.display().to_string(),
        ])
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

#[derive(Debug, Clone, serde::Serialize)]
pub struct RemoveResult {
    pub ok: bool,
    pub id: String,
    pub path: String,
    pub message: String,
    /// True if the deleted folder was the currently open serve/cwd path.
    pub was_active: bool,
}

/// Delete one cached clone under `~/.timeforge/repos` only.
/// Accepts `owner/repo`, URL, or cache id `Owner_Name`.
pub fn remove_cached(spec: &str) -> Result<RemoveResult, String> {
    remove_cached_ex(spec, None)
}

pub fn remove_cached_ex(spec: &str, active: Option<&Path>) -> Result<RemoveResult, String> {
    let target = resolve_cache_path(spec)?;
    let root = cache_dir()?
        .canonicalize()
        .unwrap_or_else(|_| cache_dir().unwrap());
    let canon = target
        .canonicalize()
        .map_err(|_| format!("not cached: {}", target.display()))?;
    if !canon.starts_with(&root) {
        return Err("refusing to delete outside ~/.timeforge/repos".into());
    }
    if !canon.join(".git").exists() {
        return Err(format!("not a git cache: {}", canon.display()));
    }
    let id = canon
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path_s = canon.display().to_string();
    let was_active = active
        .map(|a| {
            let ac = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
            ac == canon || ac.starts_with(&canon)
        })
        .unwrap_or(false);

    fs::remove_dir_all(&canon).map_err(|e| format!("delete failed: {e}"))?;

    Ok(RemoveResult {
        ok: true,
        id: id.clone(),
        path: path_s,
        message: format!("removed cached `{id}` from disk"),
        was_active,
    })
}

/// Remove every cached repo under `~/.timeforge/repos`.
pub fn clean_cached() -> Result<Vec<RemoveResult>, String> {
    let list = list_cached()?;
    let mut out = Vec::new();
    for r in list {
        out.push(remove_cached(&r.id)?);
    }
    Ok(out)
}

fn resolve_cache_path(spec: &str) -> Result<PathBuf, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("provide owner/repo or cache id".into());
    }
    let root = cache_dir()?;
    if let Ok((owner, name)) = parse_github_spec(spec) {
        return Ok(root.join(format!("{owner}_{name}")));
    }
    // Cache folder id: Owner_Name
    let by_id = root.join(spec);
    if by_id.exists() {
        return Ok(by_id);
    }
    Err(format!(
        "could not resolve `{spec}` — use owner/repo or id from `timeforge repos`"
    ))
}

pub fn is_remote_spec(spec: &str) -> bool {
    let s = spec.trim();
    if Path::new(s).exists() {
        return false;
    }
    parse_github_spec(s).is_ok()
}

/// Repair cached remote: always in the same folder — fetch/materialize, never wipe-first.
pub fn repair_cache(owner: &str, name: &str) -> Result<Repo, String> {
    let cache = cache_dir()?.join(format!("{owner}_{name}"));
    if cache.join(".git").exists() {
        eprintln!("Timeforge: repairing {owner}/{name} in place at {}", cache.display());
        let repo = Repo::discover(&cache)?;
        let _ = sync_inplace(&repo, true)?;
        return Repo::discover(&cache);
    }
    open_github(owner, name, false, true)
}

/// Core in-place sync: same directory, only download new/missing objects.
pub fn sync_inplace(repo: &Repo, repair: bool) -> Result<UpdateResult, String> {
    let path = repo.path().to_path_buf();
    let path_before = path.display().to_string();
    let before = git::git_in(&path, &["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|_| "?".into())
        .trim()
        .to_string();

    let remotes = git::git_network_in(&path, &["remote"]).unwrap_or_default();
    if remotes.trim().is_empty() {
        return Ok(UpdateResult {
            ok: true,
            before: before.clone(),
            after: before,
            message: "no remotes — nothing to fetch (path unchanged)".into(),
            partial: false,
            same_path: true,
            mode: "noop".into(),
            path: path_before,
        });
    }

    let mut notes = Vec::new();
    let mut mode = "inplace-fetch".to_string();
    let partial = git::is_partial_clone(&path);

    if repair || partial {
        match git::materialize_partial(&path) {
            Ok(m) => {
                notes.push(m);
                mode = "inplace-materialize".into();
            }
            Err(e) => {
                if repair {
                    notes.push(format!("materialize soft-fail: {e}"));
                }
            }
        }
    }

    // Incremental: only new objects for existing refs
    git::git_network_in(&path, &["fetch", "--all", "--prune"])
        .map_err(|e| format!("in-place fetch failed (path kept): {e}"))?;

    let pull = git::git_network_in(&path, &["pull", "--ff-only"]);
    let after = git::git_in(&path, &["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|_| before.clone())
        .trim()
        .to_string();

    // Path must be identical — never swap directories
    let path_after = repo.path().display().to_string();
    if path_before != path_after {
        return Err(format!(
            "internal error: path changed during sync ({path_before} → {path_after})"
        ));
    }
    if !path.join(".git").exists() {
        return Err("internal error: .git missing after sync".into());
    }

    let message = match pull {
        Ok(out) => {
            let t = out.trim();
            let core = if before == after {
                if t.is_empty() || t.contains("Already up to date") || t.contains("up to date") {
                    "already up to date (in place)".into()
                } else {
                    format!("fetched in place · HEAD {after}")
                }
            } else {
                format!("updated in place {before} → {after}")
            };
            if notes.is_empty() {
                core
            } else {
                format!("{core} · {}", notes.join(" · "))
            }
        }
        Err(e) => {
            // Fetch may still have filled objects even if pull can't ff
            if before != after {
                format!("fetched in place; pull note: {e}")
            } else if repair {
                format!("fetched/materialized in place · HEAD {after} · pull: {e}")
            } else {
                return Err(format!("update failed (repo path unchanged): {e}"));
            }
        }
    };

    Ok(UpdateResult {
        ok: true,
        before,
        after,
        message,
        partial: git::is_partial_clone(&path),
        same_path: true,
        mode,
        path: path_before,
    })
}

pub fn update_repo(repo: &Repo) -> Result<UpdateResult, String> {
    sync_inplace(repo, false)
}

/// Repair current clone **in the same directory** (no wipe / no second download root).
pub fn repair_current(repo: &Repo) -> Result<UpdateResult, String> {
    let path = repo.path().to_path_buf();
    let _ = git::git_network_in(&path, &["remote", "get-url", "origin"]).map_err(|_| {
        "no origin remote — cannot repair in place".to_string()
    })?;
    let mut result = sync_inplace(repo, true)?;
    // Verify history is usable after repair
    if git::git_in(&path, &["log", "-1", "--oneline"]).is_err() {
        return Err(format!(
            "in-place repair finished but history still broken at {}. Check network / origin.",
            path.display()
        ));
    }
    result.message = format!("repaired in place · {}", result.message);
    result.mode = if result.mode == "inplace-fetch" {
        "inplace-repair".into()
    } else {
        result.mode
    };
    Ok(result)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateResult {
    pub ok: bool,
    pub before: String,
    pub after: String,
    pub message: String,
    pub partial: bool,
    /// Always true for update/repair — path never moves.
    pub same_path: bool,
    pub mode: String,
    pub path: String,
}
