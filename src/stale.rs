use std::collections::HashMap;

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

fn skip_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".lock")
        || lower.contains("node_modules/")
        || lower.contains("/target/")
        || lower.ends_with(".png")
        || lower.ends_with(".jpg")
}

/// Last-touch ages via one `git log --name-only` pass (not N× `git log -1`).
pub fn stale_files(repo: &Repo, older_than_days: i64, limit: usize) -> Result<StaleReport, String> {
    let ls = git::git_in(repo.path(), &["ls-files"])?;
    let mut tracked: HashMap<&str, ()> = HashMap::new();
    for path in ls.lines().take(2000) {
        if path.is_empty() || skip_path(path) {
            continue;
        }
        tracked.insert(path, ());
    }
    if tracked.is_empty() {
        return Ok(StaleReport {
            older_than_days,
            files: Vec::new(),
        });
    }

    let out = git::git_in(
        repo.path(),
        &[
            "log",
            "--name-only",
            "--pretty=format:%h%x1f%an%x1f%ae%x1f%at%x1f%s",
        ],
    )?;

    let now = chrono::Utc::now().timestamp();
    let mut last_touch: HashMap<String, StaleFile> = HashMap::new();
    let rows = git::parse_name_only_log(&out);

    for (commit, paths) in rows {
        let ts: i64 = commit.date.parse().unwrap_or(0);
        let age_days = ((now - ts) / 86400).max(0);
        let date = chrono::DateTime::from_timestamp(ts, 0)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        for path in paths {
            if !tracked.contains_key(path.as_str()) {
                continue;
            }
            // Newest-first log: first sighting is last touch.
            last_touch.entry(path.clone()).or_insert_with(|| StaleFile {
                path,
                last_commit: commit.short.clone(),
                last_author: commit.author.clone(),
                last_date: date.clone(),
                age_days,
            });
            if last_touch.len() >= tracked.len() {
                break;
            }
        }
        if last_touch.len() >= tracked.len() {
            break;
        }
    }

    let mut files: Vec<StaleFile> = last_touch
        .into_values()
        .filter(|f| f.age_days >= older_than_days)
        .collect();
    files.sort_by_key(|b| std::cmp::Reverse(b.age_days));
    files.truncate(limit);

    Ok(StaleReport {
        older_than_days,
        files,
    })
}
