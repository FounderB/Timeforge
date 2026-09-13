use serde::Serialize;

use crate::git::{self, CommitInfo, PRETTY_COMMIT};
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct PrFile {
    pub path: String,
    pub insertions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaterTouch {
    pub commit: CommitInfo,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrTravel {
    pub query: String,
    pub pr: Option<String>,
    pub commit: CommitInfo,
    pub files: Vec<PrFile>,
    pub authors: Vec<String>,
    pub later: Vec<LaterTouch>,
    pub summary: String,
}

/// Time-travel a PR / issue number or merge subject fragment.
pub fn pr_travel(repo: &Repo, query: &str, later_limit: usize) -> Result<PrTravel, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("provide a PR number like 12 or #12".into());
    }
    let num = normalize_pr(q);
    if num.is_empty() {
        return Err("could not parse PR number".into());
    }
    let needle = format!("#{num}");

    let out = git::git_in(
        repo.path(),
        &[
            "log",
            "--all",
            &format!("--pretty=format:{PRETTY_COMMIT}"),
            "--date=short",
            "-i",
            "--grep",
            &needle,
            "-n",
            "40",
        ],
    )?;

    let mut best: Option<(i32, CommitInfo)> = None;
    for line in out.lines() {
        if let Some(c) = git::parse_log_line(line) {
            let subj = c.subject.to_lowercase();
            let score = if subj.contains(&format!("(#{num})")) {
                3
            } else if subj.contains("merge") && subj.contains(&needle.to_lowercase()) {
                2
            } else if subj.contains(&needle.to_lowercase()) {
                1
            } else {
                0
            };
            if score == 0 {
                continue;
            }
            match &best {
                Some((s, _)) if *s >= score => {}
                _ => best = Some((score, c)),
            }
            if score >= 3 {
                break;
            }
        }
    }

    let (_, commit) = best.ok_or_else(|| {
        format!("no commit mentioning {needle} — try a merge commit subject or different number")
    })?;

    let show = git::git_in(
        repo.path(),
        &["show", "--pretty=format:", "--numstat", &commit.hash],
    )?;

    let mut files = Vec::new();
    for line in show.lines() {
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let ins = parts[0].parse().unwrap_or(0);
            let del = parts[1].parse().unwrap_or(0);
            files.push(PrFile {
                path: parts[2].to_string(),
                insertions: ins,
                deletions: del,
            });
        }
    }
    files.sort_by_key(|b| std::cmp::Reverse(b.insertions + b.deletions));

    let authors = vec![commit.author.clone()];

    let mut later = Vec::new();
    let sample: Vec<&str> = files.iter().take(12).map(|f| f.path.as_str()).collect();
    if !sample.is_empty() {
        let mut args: Vec<String> = vec![
            "log".into(),
            format!("{}..HEAD", commit.hash),
            format!("--pretty=format:{PRETTY_COMMIT}"),
            "--date=short".into(),
            format!("-n{}", later_limit.max(5) * 3),
            "--name-only".into(),
            "--".into(),
        ];
        for p in &sample {
            args.push((*p).to_string());
        }
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        if let Ok(log) = git::git_in(repo.path(), &refs) {
            for (c, files) in git::parse_name_only_log(&log) {
                for line in files {
                    if sample.iter().any(|p| *p == line) && later.len() < later_limit {
                        later.push(LaterTouch {
                            commit: c.clone(),
                            path: line,
                        });
                    }
                }
            }
        }
    }

    let summary = format!(
        "{} · {} files · +{} /-{} · {} later touches on same paths",
        needle,
        files.len(),
        files.iter().map(|f| f.insertions).sum::<u32>(),
        files.iter().map(|f| f.deletions).sum::<u32>(),
        later.len()
    );

    Ok(PrTravel {
        query: q.to_string(),
        pr: Some(needle),
        commit,
        files,
        authors,
        later,
        summary,
    })
}

fn normalize_pr(q: &str) -> String {
    let s = q.trim().trim_start_matches('#');
    if let Some(rest) = s.strip_prefix("pull/") {
        return rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    }
    if let Some(idx) = s.rfind('/') {
        let last = &s[idx + 1..];
        if !last.is_empty() && last.chars().all(|c| c.is_ascii_digit()) {
            return last.to_string();
        }
    }
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        s.to_string()
    } else {
        digits
    }
}
