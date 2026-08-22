use std::path::{Path, PathBuf};
use std::process::Command;

pub fn rev_parse_show_toplevel(start: &Path) -> Result<PathBuf, String> {
    let out = git_in(start, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(out.trim()))
}

fn run_git(cwd: &Path, args: &[&str], no_lazy: bool) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    if no_lazy {
        cmd.env("GIT_NO_LAZY_FETCH", "1");
    }
    let output = cmd
        .output()
        .map_err(|e| format!("git failed to start: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(err.trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn is_missing_object_err(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("promisor")
        || m.contains("lazy fetch")
        || m.contains("couldn't connect")
        || m.contains("could not resolve")
        || m.contains("unable to access")
        || m.contains("не удалось")
        || m.contains("could not fetch")
        || m.contains("could not fetch")
        || m.contains("failed to fetch")
        || m.contains("object not found")
        || m.contains("bad object")
        || m.contains("filter")
}

fn format_promisor_err(msg: &str) -> String {
    format!(
        "PROMISOR: missing git objects in this clone. Click Update or Repair clone in the UI, or run: timeforge open owner/repo --repair\nDetails: {msg}"
    )
}

/// Prefer offline; on missing objects / promisor, retry with network lazy-fetch.
pub fn git_in(cwd: &Path, args: &[&str]) -> Result<String, String> {
    match run_git(cwd, args, true) {
        Ok(s) => Ok(s),
        Err(e) if is_missing_object_err(&e) => match run_git(cwd, args, false) {
            Ok(s) => Ok(s),
            Err(e2) => Err(format_promisor_err(&e2)),
        },
        Err(e) => {
            if is_missing_object_err(&e) {
                Err(format_promisor_err(&e))
            } else {
                Err(format!("git {}: {e}", args.join(" ")))
            }
        }
    }
}

pub fn rel_path(root: &Path, path: &Path) -> Result<String, String> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    let abs = abs.canonicalize().unwrap_or(abs);
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    abs.strip_prefix(&root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .map_err(|_| format!("{} is outside repo {}", path.display(), root.display()))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CommitInfo {
    pub hash: String,
    pub short: String,
    pub author: String,
    pub email: String,
    pub date: String,
    pub subject: String,
}

pub fn parse_log_line(line: &str) -> Option<CommitInfo> {
    let parts: Vec<&str> = line.splitn(5, '|').collect();
    if parts.len() < 5 {
        return None;
    }
    Some(CommitInfo {
        hash: parts[0].to_string(),
        short: parts[0].chars().take(8).collect(),
        author: parts[1].to_string(),
        email: parts[2].to_string(),
        date: parts[3].to_string(),
        subject: parts[4].to_string(),
    })
}

/// Run git allowing network (fetch/pull/materialize).
pub fn git_network_in(cwd: &Path, args: &[&str]) -> Result<String, String> {
    run_git(cwd, args, false).map_err(|e| format!("git {}: {e}", args.join(" ")))
}

pub fn is_partial_clone(repo: &Path) -> bool {
    let filter = run_git(repo, &["config", "--get", "remote.origin.partialclonefilter"], false)
        .unwrap_or_default();
    let promisor = run_git(repo, &["config", "--get", "remote.origin.promisor"], false)
        .unwrap_or_default();
    let ext = run_git(repo, &["config", "--get", "extensions.partialclone"], false)
        .unwrap_or_default();
    !filter.trim().is_empty()
        || promisor.trim() == "true"
        || !ext.trim().is_empty()
}

/// Try to download missing objects for a partial/promisor clone.
pub fn materialize_partial(repo: &Path) -> Result<String, String> {
    let _ = run_git(repo, &["config", "--unset", "remote.origin.promisor"], false);
    let _ = run_git(
        repo,
        &["config", "--unset", "remote.origin.partialclonefilter"],
        false,
    );
    let _ = run_git(repo, &["config", "--unset", "extensions.partialclone"], false);
    // Prefer a full refetch of reachable objects.
    let refetch = git_network_in(repo, &["fetch", "--refetch", "--prune"]);
    if refetch.is_ok() {
        return Ok("fetched missing objects (--refetch)".into());
    }
    let all = git_network_in(repo, &["fetch", "--all", "--prune"])?;
    Ok(if all.trim().is_empty() {
        "fetched remotes".into()
    } else {
        all.trim().chars().take(120).collect()
    })
}

/// Soften promisor flags without requiring a full re-clone.
pub fn harden_local_clone(repo: &Path) {
    // Do NOT force GIT_NO_LAZY forever on partial clones — that makes analysis unusable.
    if is_partial_clone(repo) {
        return;
    }
    let _ = run_git(repo, &["config", "remote.origin.promisor", "false"], false);
}

/// Quick network probe (ls-remote) with timeout. Does not change the repo.
pub fn probe_network(repo: &Path, timeout_ms: u64) -> NetworkProbe {
    let has_remote = run_git(repo, &["remote"], false)
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if !has_remote {
        return NetworkProbe {
            online: false,
            has_remote: false,
            detail: "no remotes".into(),
            ms: 0,
        };
    }
    let repo = repo.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let t0 = std::time::Instant::now();
        let r = run_git(&repo, &["ls-remote", "--heads", "origin"], false);
        let _ = tx.send((r, t0.elapsed().as_millis() as u64));
    });
    match rx.recv_timeout(std::time::Duration::from_millis(timeout_ms.max(500))) {
        Ok((Ok(_), ms)) => NetworkProbe {
            online: true,
            has_remote: true,
            detail: "origin reachable".into(),
            ms,
        },
        Ok((Err(e), ms)) => NetworkProbe {
            online: false,
            has_remote: true,
            detail: e.chars().take(100).collect(),
            ms,
        },
        Err(_) => NetworkProbe {
            online: false,
            has_remote: true,
            detail: format!("timeout after {timeout_ms}ms"),
            ms: timeout_ms,
        },
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkProbe {
    pub online: bool,
    pub has_remote: bool,
    pub detail: String,
    pub ms: u64,
}
