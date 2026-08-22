use serde::Serialize;

use crate::git::{self, CommitInfo, PRETTY_COMMIT};
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct FixBreakPair {
    pub fix: CommitInfo,
    pub break_commit: Option<CommitInfo>,
    pub shared_files: Vec<String>,
    pub confidence: u32,
    pub note: String,
    /// pickaxe | blame | cochange
    pub method: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixBreakReport {
    pub pairs: Vec<FixBreakPair>,
    pub summary: String,
}

/// Pair fix/revert commits with the commit that *introduced* the code the fix removed.
///
/// Causality: extract removed hunks from the fix → `git log -S` for when that text appeared.
pub fn fix_break_pairs(repo: &Repo, since: &str, limit: usize) -> Result<FixBreakReport, String> {
    fix_break_pairs_ex(repo, since, limit, false)
}

pub fn fix_break_pairs_ex(
    repo: &Repo,
    since: &str,
    limit: usize,
    fast: bool,
) -> Result<FixBreakReport, String> {
    let scan = if fast { 16 } else { 40 };
    let out = git::git_in(
        repo.path(),
        &[
            "log",
            &format!("--since={since}"),
            &format!("--pretty=format:{PRETTY_COMMIT}"),
            "--date=short",
            "-i",
            "-E",
            "--grep",
            "fix|bug|hotfix|revert|regress|patch",
            "-n",
            &scan.to_string(),
            "--name-only",
        ],
    )?;

    let fixes: Vec<(CommitInfo, Vec<String>)> = git::parse_name_only_log(&out)
        .into_iter()
        .filter(|(c, _)| is_fixish(&c.subject) && !is_noise_fix(&c.subject))
        .collect();

    let mut pairs = Vec::new();
    for (fix, touched) in fixes {
        if pairs.len() >= limit {
            break;
        }
        let pair = resolve_break(repo, &fix, &touched, fast);
        // In fast mode skip weak cochange noise
        if fast && (pair.method == "cochange" || pair.method == "none") {
            continue;
        }
        pairs.push(pair);
    }

    pairs.sort_by(|a, b| b.confidence.cmp(&a.confidence));

    let summary = format!(
        "{} fix↔break · {} via pickaxe/blame · {} weak",
        pairs.len(),
        pairs
            .iter()
            .filter(|p| p.break_commit.is_some() && p.method != "cochange")
            .count(),
        pairs
            .iter()
            .filter(|p| p.method == "cochange" || p.break_commit.is_none())
            .count()
    );

    Ok(FixBreakReport { pairs, summary })
}

fn resolve_break(repo: &Repo, fix: &CommitInfo, touched: &[String], fast: bool) -> FixBreakPair {
    // 1) Pickaxe on removed hunk text
    if let Some((brk, files, needle)) = pickaxe_introducer(repo, &fix.hash, fast) {
        return FixBreakPair {
            fix: fix.clone(),
            break_commit: Some(brk),
            shared_files: files,
            confidence: 88,
            note: format!("introduced code later removed by fix (−S `{needle}`)"),
            method: "pickaxe".into(),
        };
    }

    // 2) Blame parent version of first touched file around changed lines
    if let Some((brk, file)) = blame_parent_touch(repo, &fix.hash, touched) {
        return FixBreakPair {
            fix: fix.clone(),
            break_commit: Some(brk),
            shared_files: vec![file],
            confidence: 70,
            note: "blame on pre-fix lines points at this commit".into(),
            method: "blame".into(),
        };
    }

    if fast {
        return FixBreakPair {
            fix: fix.clone(),
            break_commit: None,
            shared_files: touched.iter().take(8).cloned().collect(),
            confidence: 15,
            note: "fix found; could not attribute introducer".into(),
            method: "none".into(),
        };
    }

    // 3) Weak fallback: recent co-change (explicitly low confidence)
    if let Some((brk, shared)) = weak_cochange(repo, &fix.hash, touched) {
        return FixBreakPair {
            fix: fix.clone(),
            break_commit: Some(brk),
            shared_files: shared,
            confidence: 35,
            note: "weak co-change heuristic — not hunk-proven".into(),
            method: "cochange".into(),
        };
    }

    FixBreakPair {
        fix: fix.clone(),
        break_commit: None,
        shared_files: touched.iter().take(8).cloned().collect(),
        confidence: 15,
        note: "fix found; could not attribute introducer".into(),
        method: "none".into(),
    }
}

fn pickaxe_introducer(
    repo: &Repo,
    fix_hash: &str,
    fast: bool,
) -> Option<(CommitInfo, Vec<String>, String)> {
    let diff = git::git_in(
        repo.path(),
        &["show", "--format=", "--unified=0", fix_hash],
    )
    .ok()?;
    let needle_cap = if fast { 3 } else { 6 };
    let needles = extract_removed_needles(&diff);
    for needle in needles.into_iter().take(needle_cap) {
        let range = format!("{fix_hash}^");
        let log = git::git_in(
            repo.path(),
            &[
                "log",
                "-S",
                &needle,
                &format!("--pretty=format:{PRETTY_COMMIT}"),
                "--date=short",
                "--reverse",
                "-n",
                "8",
                &range,
            ],
        )
        .unwrap_or_default();
        let mut first: Option<CommitInfo> = None;
        let mut files = Vec::new();
        for line in log.lines() {
            if let Some(c) = git::parse_log_line(line) {
                if first.is_none() {
                    first = Some(c);
                }
            } else if first.is_some() && !line.is_empty() && !line.contains('\x1f') {
                if !files.contains(&line.to_string()) {
                    files.push(line.to_string());
                }
            }
        }
        // Prefer reverse log without name-only — get files via show
        if let Some(c) = first {
            let shown = git::git_in(
                repo.path(),
                &[
                    "show",
                    "--pretty=format:",
                    "--name-only",
                    "--",
                    &c.hash,
                ],
            )
            .unwrap_or_default();
            let mut f: Vec<String> = shown
                .lines()
                .filter(|l| !l.is_empty())
                .map(|s| s.to_string())
                .collect();
            if f.is_empty() {
                f = files;
            }
            let preview: String = needle.chars().take(48).collect();
            return Some((c, f, preview));
        }
    }
    None
}

fn extract_removed_needles(diff: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in diff.lines() {
        if !line.starts_with('-') || line.starts_with("---") {
            continue;
        }
        let body = line[1..].trim();
        if !is_codeish_needle(body) {
            continue;
        }
        out.push(body.to_string());
    }
    out.sort_by(|a, b| b.len().cmp(&a.len()));
    out.dedup();
    out
}

fn is_codeish_needle(body: &str) -> bool {
    if body.len() < 12 || body.len() > 120 {
        return false;
    }
    if body.starts_with("//")
        || body.starts_with('#')
        || body.starts_with('*')
        || body.starts_with("<!--")
    {
        return false;
    }
    // Markup / man-page / roff / markdown noise — not causal code
    if (body.contains('<') && body.contains('>'))
        || body.contains("\\f")
        || body.contains("\\-")
        || body.contains("\\&")
        || body.contains("]]>")
        || body.contains("](")
        || body.contains("http://")
        || body.contains("https://")
        || body.starts_with("- [")
        || body.starts_with("* [")
    {
        return false;
    }
    let alnum = body.chars().filter(|c| c.is_alphanumeric()).count();
    if alnum < 8 {
        return false;
    }
    // Prefer lines that look like identifiers / calls, not prose
    let has_ident = body.contains('_')
        || body.contains('(')
        || body.contains("::")
        || body.contains('.')
        || body.chars().any(|c| c.is_ascii_uppercase())
            && body.chars().any(|c| c.is_ascii_lowercase());
    if !has_ident && body.split_whitespace().count() > 8 {
        return false; // long prose
    }
    true
}

fn is_noise_fix(subject: &str) -> bool {
    let s = subject.to_lowercase();
    let noise = ["typo", "readme", "changelog", "whitespace", "formatting", "clippy"];
    if noise.iter().any(|w| s.contains(w)) {
        // Keep if it also looks like a real bugfix
        return !["panic", "deadlock", "crash", "secur", "overflow", "race", "null"]
            .iter()
            .any(|w| s.contains(w));
    }
    if s.starts_with("doc:") || s.starts_with("docs:") || s.starts_with("ci:") {
        return !s.contains("panic") && !s.contains("deadlock");
    }
    false
}

fn blame_parent_touch(
    repo: &Repo,
    fix_hash: &str,
    touched: &[String],
) -> Option<(CommitInfo, String)> {
    let file = touched.first()?;
    let parent = format!("{fix_hash}^");
    // Which lines changed in this file?
    let diff = git::git_in(
        repo.path(),
        &[
            "show",
            "--format=",
            "--unified=0",
            fix_hash,
            "--",
            file,
        ],
    )
    .ok()?;
    let mut line_no: Option<u32> = None;
    for l in diff.lines() {
        // @@ -12,0 +12,2 @@  or @@ -12 +12 @@
        if let Some(rest) = l.strip_prefix("@@ ") {
            if let Some(minus) = rest.split_whitespace().next() {
                let n = minus
                    .trim_start_matches('-')
                    .split(',')
                    .next()
                    .and_then(|s| s.parse().ok());
                if let Some(n) = n {
                    if n > 0 {
                        line_no = Some(n);
                        break;
                    }
                }
            }
        }
    }
    let ln = line_no.unwrap_or(1);
    let blame = git::git_in(
        repo.path(),
        &[
            "blame",
            "-L",
            &format!("{ln},+1"),
            "--line-porcelain",
            &parent,
            "--",
            file,
        ],
    )
    .ok()?;
    let mut hash = String::new();
    let mut author = String::new();
    let mut date = String::new();
    let mut subject = String::new();
    for l in blame.lines() {
        if l.len() >= 40 && l.as_bytes().iter().take(40).all(|b| b.is_ascii_hexdigit()) {
            hash = l.split_whitespace().next()?.to_string();
        } else if let Some(a) = l.strip_prefix("author ") {
            author = a.to_string();
        } else if let Some(t) = l.strip_prefix("author-time ") {
            if let Ok(ts) = t.parse::<i64>() {
                date = chrono::DateTime::from_timestamp(ts, 0)
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_default();
            }
        } else if let Some(s) = l.strip_prefix("summary ") {
            subject = s.to_string();
        }
    }
    if hash.is_empty() || hash.chars().all(|c| c == '0') {
        return None;
    }
    Some((
        CommitInfo {
            short: hash.chars().take(8).collect(),
            hash,
            author,
            email: String::new(),
            date,
            subject,
        },
        file.clone(),
    ))
}

fn weak_cochange(
    repo: &Repo,
    fix_hash: &str,
    touched: &[String],
) -> Option<(CommitInfo, Vec<String>)> {
    let sample: Vec<&str> = touched.iter().take(6).map(|s| s.as_str()).collect();
    if sample.is_empty() {
        return None;
    }
    let mut args: Vec<String> = vec![
        "log".into(),
        format!("{fix_hash}^"),
        format!("--pretty=format:{PRETTY_COMMIT}"),
        "--date=short".into(),
        "-n".into(),
        "15".into(),
        "--name-only".into(),
        "--".into(),
    ];
    for p in &sample {
        args.push((*p).to_string());
    }
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let earlier = git::git_in(repo.path(), &refs).ok()?;
    let rows = git::parse_name_only_log(&earlier);
    let (c, files) = rows.into_iter().next()?;
    let shared: Vec<String> = files
        .into_iter()
        .filter(|f| sample.iter().any(|s| s == f))
        .collect();
    if shared.is_empty() {
        return None;
    }
    Some((c, shared))
}

fn is_fixish(subject: &str) -> bool {
    let s = subject.to_lowercase();
    ["fix", "bug", "hotfix", "revert", "regress", "patch"]
        .iter()
        .any(|w| s.contains(w))
}
