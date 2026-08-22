use serde::Serialize;

use crate::git::{self, CommitInfo};
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub commit: CommitInfo,
    pub insertions: u32,
    pub deletions: u32,
    pub pr_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Timeline {
    pub path: String,
    pub events: Vec<TimelineEvent>,
    pub total_commits: usize,
    pub first_author: Option<String>,
    pub last_author: Option<String>,
}

pub fn file_timeline(repo: &Repo, path: &str, limit: usize) -> Result<Timeline, String> {
    let rel = if PathLike(path).is_repo_relative() {
        path.to_string()
    } else {
        git::rel_path(repo.path(), std::path::Path::new(path))?
    };

    let fmt = "%H|%an|%ae|%ad|%s";
    // Prefer numstat + rename follow; fall back if promisor/offline clone can't fetch blobs.
    let with_stat = git::git_in(
        repo.path(),
        &[
            "log",
            "--follow",
            &format!("--pretty=format:{fmt}"),
            "--date=short",
            &format!("-n{limit}"),
            "--numstat",
            "--",
            &rel,
        ],
    );
    let out = match with_stat {
        Ok(s) => s,
        Err(_) => git::git_in(
            repo.path(),
            &[
                "log",
                "--follow",
                &format!("--pretty=format:{fmt}"),
                "--date=short",
                &format!("-n{limit}"),
                "--",
                &rel,
            ],
        )
        .or_else(|_| {
            git::git_in(
                repo.path(),
                &[
                    "log",
                    &format!("--pretty=format:{fmt}"),
                    "--date=short",
                    &format!("-n{limit}"),
                    "--",
                    &rel,
                ],
            )
        })?,
    };

    let mut events = Vec::new();
    let mut current: Option<CommitInfo> = None;
    let mut ins = 0u32;
    let mut del = 0u32;

    for line in out.lines() {
        if line.is_empty() {
            continue;
        }
        if let Some(c) = git::parse_log_line(line) {
            if let Some(prev) = current.take() {
                events.push(make_event(prev, ins, del));
                ins = 0;
                del = 0;
            }
            current = Some(c);
        } else if let Some((a, b, _)) = parse_numstat(line) {
            ins += a;
            del += b;
        }
    }
    if let Some(prev) = current {
        events.push(make_event(prev, ins, del));
    }

    let first_author = events.last().map(|e| e.commit.author.clone());
    let last_author = events.first().map(|e| e.commit.author.clone());
    let total = events.len();

    Ok(Timeline {
        path: rel,
        events,
        total_commits: total,
        first_author,
        last_author,
    })
}

fn make_event(commit: CommitInfo, insertions: u32, deletions: u32) -> TimelineEvent {
    let pr_hint = extract_pr_hint(&commit.subject);
    TimelineEvent {
        commit,
        insertions,
        deletions,
        pr_hint,
    }
}

fn extract_pr_hint(subject: &str) -> Option<String> {
    // (#123) or Merge pull request #123
    if let Some(i) = subject.rfind("(#") {
        let rest = &subject[i + 2..];
        let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !num.is_empty() {
            return Some(format!("#{num}"));
        }
    }
    if let Some(i) = subject.find('#') {
        let rest = &subject[i + 1..];
        let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !num.is_empty() {
            return Some(format!("#{num}"));
        }
    }
    None
}

fn parse_numstat(line: &str) -> Option<(u32, u32, String)> {
    let parts: Vec<&str> = line.split('\t').collect();
    if parts.len() < 3 {
        return None;
    }
    let ins = parts[0].parse().ok().unwrap_or(0);
    let del = parts[1].parse().ok().unwrap_or(0);
    Some((ins, del, parts[2].to_string()))
}

struct PathLike<'a>(&'a str);
impl PathLike<'_> {
    fn is_repo_relative(&self) -> bool {
        !self.0.starts_with('/') && !self.0.contains(":\\")
    }
}
