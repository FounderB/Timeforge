use std::collections::HashMap;

use serde::Serialize;

use crate::git::{self, CommitInfo};
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct Suspect {
    pub commit: CommitInfo,
    pub score: u32,
    pub reasons: Vec<String>,
    pub files_touched: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WhyReport {
    pub query: String,
    pub path_filter: Option<String>,
    pub suspects: Vec<Suspect>,
    pub hint: String,
}

pub fn why_broke(
    repo: &Repo,
    path: Option<&str>,
    since: &str,
    keyword: Option<&str>,
    limit: usize,
) -> Result<WhyReport, String> {
    let mut args = vec![
        "log".to_string(),
        format!("--since={since}"),
        "--pretty=format:%H|%an|%ae|%ad|%s".to_string(),
        "--date=short".to_string(),
        "-n".to_string(),
        "80".to_string(),
        "--name-only".to_string(),
    ];
    if let Some(p) = path {
        args.push("--".into());
        args.push(p.into());
    }

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = git::git_in(repo.path(), &arg_refs)?;

    let mut suspects = Vec::new();
    let mut current: Option<CommitInfo> = None;
    let mut files: Vec<String> = Vec::new();

    for line in out.lines() {
        if line.is_empty() {
            if let Some(c) = current.take() {
                suspects.push(score_commit(c, &files, keyword));
                files.clear();
            }
            continue;
        }
        if let Some(c) = git::parse_log_line(line) {
            if let Some(prev) = current.take() {
                suspects.push(score_commit(prev, &files, keyword));
                files.clear();
            }
            current = Some(c);
        } else if !line.contains('|') {
            files.push(line.to_string());
        }
    }
    if let Some(c) = current {
        suspects.push(score_commit(c, &files, keyword));
    }

    suspects.sort_by(|a, b| b.score.cmp(&a.score));
    suspects.truncate(limit);

    let query = keyword.unwrap_or("recent risk").to_string();
    Ok(WhyReport {
        query,
        path_filter: path.map(|s| s.to_string()),
        suspects,
        hint: "Higher score = likelier culprit. Re-run tests on the top commit: git checkout <hash> && cargo test".into(),
    })
}

fn score_commit(commit: CommitInfo, files: &[String], keyword: Option<&str>) -> Suspect {
    let mut score = 10u32;
    let mut reasons = Vec::new();

    let subj = commit.subject.to_lowercase();
    for word in ["fix", "bug", "hotfix", "break", "revert", "wip", "temp", "hack"] {
        if subj.contains(word) {
            score += 15;
            reasons.push(format!("subject contains `{word}`"));
        }
    }
    if let Some(kw) = keyword {
        let k = kw.to_lowercase();
        if subj.contains(&k) || files.iter().any(|f| f.to_lowercase().contains(&k)) {
            score += 25;
            reasons.push(format!("matches keyword `{kw}`"));
        }
    }
    if files.len() > 12 {
        score += 10;
        reasons.push(format!("large blast ({} files)", files.len()));
    } else if files.len() == 1 {
        score += 5;
        reasons.push("single-file change (easy to isolate)".into());
    }
    for f in files {
        let fl = f.to_lowercase();
        if fl.contains("auth") || fl.contains("security") || fl.contains("crypto") || fl.contains("payment") {
            score += 12;
            reasons.push(format!("touches sensitive path `{f}`"));
            break;
        }
    }
    if reasons.is_empty() {
        reasons.push("recent change in scope".into());
    }

    let mut files_touched = files.to_vec();
    files_touched.truncate(12);

    Suspect {
        commit,
        score,
        reasons,
        files_touched,
    }
}

pub fn co_changed_files(repo: &Repo, path: &str, limit: usize) -> Result<HashMap<String, u32>, String> {
    let rel = git::rel_path(repo.path(), std::path::Path::new(path))
        .unwrap_or_else(|_| path.replace('\\', "/"));
    let out = git::git_in(
        repo.path(),
        &[
            "log",
            "--pretty=format:--",
            "--name-only",
            "-n",
            "60",
            "--",
            &rel,
        ],
    )?;

    let mut counts: HashMap<String, u32> = HashMap::new();
    let mut batch: Vec<String> = Vec::new();
    for line in out.lines() {
        if line == "--" {
            for f in &batch {
                if f != &rel {
                    *counts.entry(f.clone()).or_insert(0) += 1;
                }
            }
            batch.clear();
        } else if !line.is_empty() {
            batch.push(line.to_string());
        }
    }
    for f in &batch {
        if f != &rel {
            *counts.entry(f.clone()).or_insert(0) += 1;
        }
    }

    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.truncate(limit);
    Ok(v.into_iter().collect())
}
