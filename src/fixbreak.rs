use serde::Serialize;

use crate::git::{self, CommitInfo};
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct FixBreakPair {
    pub fix: CommitInfo,
    pub break_commit: Option<CommitInfo>,
    pub shared_files: Vec<String>,
    pub confidence: u32,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixBreakReport {
    pub pairs: Vec<FixBreakPair>,
    pub summary: String,
}

/// Find fix/revert commits and the earlier change on the same paths that likely broke things.
pub fn fix_break_pairs(repo: &Repo, since: &str, limit: usize) -> Result<FixBreakReport, String> {
    let out = git::git_in(
        repo.path(),
        &[
            "log",
            &format!("--since={since}"),
            "--pretty=format:%H|%an|%ae|%ad|%s",
            "--date=short",
            "-i",
            "-E",
            "--grep",
            "fix|bug|hotfix|revert|regress|patch",
            "-n",
            "60",
            "--name-only",
        ],
    )?;

    let mut fixes: Vec<(CommitInfo, Vec<String>)> = Vec::new();
    let mut current: Option<CommitInfo> = None;
    let mut files: Vec<String> = Vec::new();

    for line in out.lines() {
        if line.is_empty() {
            if let Some(c) = current.take() {
                if is_fixish(&c.subject) {
                    fixes.push((c, std::mem::take(&mut files)));
                } else {
                    files.clear();
                }
            }
            continue;
        }
        if let Some(c) = git::parse_log_line(line) {
            if let Some(prev) = current.take() {
                if is_fixish(&prev.subject) {
                    fixes.push((prev, std::mem::take(&mut files)));
                } else {
                    files.clear();
                }
            }
            current = Some(c);
        } else if !line.contains('|') {
            files.push(line.to_string());
        }
    }
    if let Some(c) = current {
        if is_fixish(&c.subject) {
            fixes.push((c, files));
        }
    }

    let mut pairs = Vec::new();
    for (fix, touched) in fixes {
        if pairs.len() >= limit {
            break;
        }
        let sample: Vec<String> = touched.into_iter().take(8).collect();
        if sample.is_empty() {
            pairs.push(FixBreakPair {
                fix,
                break_commit: None,
                shared_files: vec![],
                confidence: 20,
                note: "fix-like commit with no file list".into(),
            });
            continue;
        }

        let mut args: Vec<String> = vec![
            "log".into(),
            format!("{}^", fix.hash),
            "--pretty=format:%H|%an|%ae|%ad|%s".into(),
            "--date=short".into(),
            "-n".into(),
            "40".into(),
            "--".into(),
        ];
        for p in &sample {
            args.push(p.clone());
        }
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let earlier = git::git_in(repo.path(), &refs).unwrap_or_default();

        let mut best: Option<(CommitInfo, u32, Vec<String>)> = None;
        let mut cur: Option<CommitInfo> = None;
        let mut cur_files: Vec<String> = Vec::new();

        for line in earlier.lines().chain(std::iter::once("")) {
            if line.is_empty() {
                if let Some(c) = cur.take() {
                    if c.hash == fix.hash {
                        cur_files.clear();
                        continue;
                    }
                    let shared: Vec<String> = cur_files
                        .iter()
                        .filter(|f| sample.iter().any(|s| s == *f))
                        .cloned()
                        .collect();
                    cur_files.clear();
                    if shared.is_empty() {
                        continue;
                    }
                    let mut conf = 30 + shared.len() as u32 * 8;
                    let subj = c.subject.to_lowercase();
                    if ["wip", "hack", "temp", "break", "refactor", "oops"]
                        .iter()
                        .any(|w| subj.contains(w))
                    {
                        conf += 15;
                    }
                    if is_fixish(&c.subject) {
                        conf = conf.saturating_sub(20);
                    }
                    match &best {
                        Some((_, b, _)) if *b >= conf => {}
                        _ => best = Some((c, conf, shared)),
                    }
                }
                continue;
            }
            if let Some(c) = git::parse_log_line(line) {
                // flush previous before switching
                if cur.is_some() {
                    // re-process flush by pushing empty - simpler: handle below
                    let prev = cur.take().unwrap();
                    let shared: Vec<String> = cur_files
                        .iter()
                        .filter(|f| sample.iter().any(|s| s == *f))
                        .cloned()
                        .collect();
                    cur_files.clear();
                    if !shared.is_empty() && prev.hash != fix.hash {
                        let mut conf = 30 + shared.len() as u32 * 8;
                        let subj = prev.subject.to_lowercase();
                        if ["wip", "hack", "temp", "break", "refactor", "oops"]
                            .iter()
                            .any(|w| subj.contains(w))
                        {
                            conf += 15;
                        }
                        if is_fixish(&prev.subject) {
                            conf = conf.saturating_sub(20);
                        }
                        match &best {
                            Some((_, b, _)) if *b >= conf => {}
                            _ => best = Some((prev, conf, shared)),
                        }
                    }
                }
                cur = Some(c);
            } else if !line.contains('|') {
                cur_files.push(line.to_string());
            }
        }

        if let Some((break_c, conf, shared)) = best {
            pairs.push(FixBreakPair {
                note: format!(
                    "likely break on {} shared path(s) before fix",
                    shared.len()
                ),
                fix,
                break_commit: Some(break_c),
                shared_files: shared,
                confidence: conf.min(99),
            });
        } else {
            pairs.push(FixBreakPair {
                fix,
                break_commit: None,
                shared_files: sample,
                confidence: 25,
                note: "fix found; no clear earlier culprit on same paths".into(),
            });
        }
    }

    pairs.sort_by(|a, b| b.confidence.cmp(&a.confidence));

    let summary = format!(
        "{} fix↔break pairs · {} with a likely culprit",
        pairs.len(),
        pairs.iter().filter(|p| p.break_commit.is_some()).count()
    );

    Ok(FixBreakReport { pairs, summary })
}

fn is_fixish(subject: &str) -> bool {
    let s = subject.to_lowercase();
    ["fix", "bug", "hotfix", "revert", "regress", "patch", "correct"]
        .iter()
        .any(|w| s.contains(w))
}
