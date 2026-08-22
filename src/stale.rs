use serde::Serialize;

use crate::git;
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct StaleFile {
    pub path: String,
    pub last_commit: String,
    pub last_date: String,
    pub last_author: String,
    pub age_days: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StaleReport {
    pub older_than_days: i64,
    pub files: Vec<StaleFile>,
}

pub fn stale_files(repo: &Repo, older_than_days: i64, limit: usize) -> Result<StaleReport, String> {
    // list tracked files
    let ls = git::git_in(repo.path(), &["ls-files"])?;
    let mut files = Vec::new();
    let now = chrono::Utc::now().timestamp();

    for path in ls.lines().take(2000) {
        if path.is_empty() {
            continue;
        }
        // skip bulky / generated
        let lower = path.to_lowercase();
        if lower.ends_with(".lock")
            || lower.contains("node_modules/")
            || lower.contains("/target/")
            || lower.ends_with(".png")
            || lower.ends_with(".jpg")
        {
            continue;
        }
        let log = git::git_in(
            repo.path(),
            &[
                "log",
                "-1",
                "--pretty=format:%h|%an|%ae|%at|%s",
                "--",
                path,
            ],
        );
        let Ok(line) = log else { continue };
        let parts: Vec<&str> = line.splitn(5, '|').collect();
        if parts.len() < 5 {
            continue;
        }
        let ts: i64 = parts[3].parse().unwrap_or(0);
        let age_days = ((now - ts) / 86400).max(0);
        if age_days < older_than_days {
            continue;
        }
        files.push(StaleFile {
            path: path.to_string(),
            last_commit: parts[0].to_string(),
            last_author: parts[1].to_string(),
            last_date: chrono::DateTime::from_timestamp(ts, 0)
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            age_days,
        });
    }

    files.sort_by(|a, b| b.age_days.cmp(&a.age_days));
    files.truncate(limit);

    Ok(StaleReport {
        older_than_days,
        files,
    })
}
