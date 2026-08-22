use std::path::{Path, PathBuf};
use std::process::Command;

/// Field separator that cannot appear in commit subjects (ASCII Unit Separator).
pub const FIELD_SEP: char = '\x1f';
/// `git log --pretty=format:` with safe separators.
pub const PRETTY_COMMIT: &str = "%H%x1f%an%x1f%ae%x1f%ad%x1f%s";

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
        || m.contains("failed to fetch")
        || m.contains("object not found")
        || m.contains("bad object")
}

fn format_promisor_err(msg: &str) -> String {
    format!(
        "PROMISOR: missing git objects in this clone. Click Update or Repair clone in the UI, or run: timeforge open owner/repo --repair\nDetails: {msg}"
    )
}

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

/// Parse one commit header line produced with [`PRETTY_COMMIT`].
pub fn parse_log_line(line: &str) -> Option<CommitInfo> {
    if line.is_empty() || line.starts_with('\t') {
        return None;
    }
    let sep = if line.matches(FIELD_SEP).count() >= 4 {
        FIELD_SEP
    } else if line.matches('|').count() >= 4 {
        '|'
    } else {
        return None;
    };
    let parts: Vec<&str> = line.splitn(5, sep).collect();
    if parts.len() < 5 {
        return None;
    }
    let hash = parts[0];
    if hash.len() < 7 || !hash.chars().take(7).all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(CommitInfo {
        hash: hash.to_string(),
        short: hash.chars().take(8).collect(),
        author: parts[1].to_string(),
        email: parts[2].to_string(),
        date: parts[3].to_string(),
        subject: parts[4].to_string(),
    })
}

/// Walk `git log --name-only` output into (commit, files).
pub fn parse_name_only_log(out: &str) -> Vec<(CommitInfo, Vec<String>)> {
    let mut rows = Vec::new();
    let mut current: Option<CommitInfo> = None;
    let mut files: Vec<String> = Vec::new();

    let flush = |cur: &mut Option<CommitInfo>, files: &mut Vec<String>, rows: &mut Vec<_>| {
        if let Some(c) = cur.take() {
            rows.push((c, std::mem::take(files)));
        }
    };

    for line in out.lines() {
        if line.is_empty() {
            flush(&mut current, &mut files, &mut rows);
            continue;
        }
        if let Some(c) = parse_log_line(line) {
            flush(&mut current, &mut files, &mut rows);
            current = Some(c);
        } else if current.is_some() {
            files.push(line.to_string());
        }
    }
    flush(&mut current, &mut files, &mut rows);
    rows
}

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

pub fn materialize_partial(repo: &Path) -> Result<String, String> {
    let _ = run_git(repo, &["config", "--unset", "remote.origin.promisor"], false);
    let _ = run_git(
        repo,
        &["config", "--unset", "remote.origin.partialclonefilter"],
        false,
    );
    let _ = run_git(repo, &["config", "--unset", "extensions.partialclone"], false);
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

pub fn harden_local_clone(repo: &Path) {
    if is_partial_clone(repo) {
        return;
    }
    let _ = run_git(repo, &["config", "remote.origin.promisor", "false"], false);
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_subject_with_pipe() {
        let line = format!(
            "abcdef0123456789{s}Dev{s}dev@ex.com{s}2024-01-01{s}fix a|b|c in parser",
            s = FIELD_SEP
        );
        let c = parse_log_line(&line).expect("parse");
        assert_eq!(c.subject, "fix a|b|c in parser");
        assert_eq!(c.author, "Dev");
    }

    #[test]
    fn legacy_pipe_format_still_parses() {
        let line = "abcdef0123456789|Dev|dev@ex.com|2024-01-01|plain subject";
        let c = parse_log_line(line).expect("parse");
        assert_eq!(c.subject, "plain subject");
    }
}
