use std::path::{Path, PathBuf};
use std::process::Command;

pub fn rev_parse_show_toplevel(start: &Path) -> Result<PathBuf, String> {
    let out = git_in(start, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(out.trim()))
}

pub fn git_in(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git failed to start: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git {}: {}", args.join(" "), err.trim()));
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
    // hash|author|email|date|subject
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
