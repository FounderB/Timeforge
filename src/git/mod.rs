use std::path::{Path, PathBuf};
use std::process::Command;

pub fn rev_parse_show_toplevel(start: &Path) -> Result<PathBuf, String> {
    let out = git_in(start, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(out.trim()))
}

/// Run git with lazy fetch disabled so partial clones don't hit the network.
pub fn git_in(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .map_err(|e| format!("git failed to start: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let msg = err.trim();
        if msg.contains("Couldn't connect")
            || msg.contains("Could not resolve")
            || msg.contains("promisor")
            || msg.contains("unable to access")
        {
            return Err(format!(
                "git needs objects that aren't cached locally (offline / promisor). Fix: timeforge open owner/repo --repair\nDetails: {msg}"
            ));
        }
        return Err(format!("git {}: {msg}", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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

/// Run git allowing network (fetch/pull). Does not set GIT_NO_LAZY_FETCH.
pub fn git_network_in(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .map_err(|e| format!("git failed to start: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {}: {}", args.join(" "), err.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Disable promisor remotes so offline analysis never tries GitHub again.
pub fn harden_local_clone(repo: &Path) {
    let _ = git_in(repo, &["config", "remote.origin.promisor", "false"]);
    let _ = Command::new("git")
        .args(["config", "extensions.partialclone", ""])
        .current_dir(repo)
        .env("GIT_NO_LAZY_FETCH", "1")
        .status();
}
