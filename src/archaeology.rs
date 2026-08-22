use serde::Serialize;

use crate::git::{self, CommitInfo, PRETTY_COMMIT};
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct DigEvent {
    pub commit: CommitInfo,
    pub files: Vec<String>,
    pub kind: String, // introduced | changed | removed-ish
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchaeologyReport {
    pub pattern: String,
    pub events: Vec<DigEvent>,
    pub first: Option<CommitInfo>,
    pub last: Option<CommitInfo>,
    pub summary: String,
}

/// When did a code pattern first appear / keep changing? (`git log -S` pickaxe).
pub fn dig_pattern(repo: &Repo, pattern: &str, limit: usize) -> Result<ArchaeologyReport, String> {
    let pat = pattern.trim();
    if pat.len() < 2 {
        return Err("pattern too short — try at least 2 characters".into());
    }

    // Prefer pickaxe (-S); fall back to regex (-G) for short tokens.
    let out = git::git_in(
        repo.path(),
        &[
            "log",
            "-S",
            pat,
            &format!("--pretty=format:{PRETTY_COMMIT}"),
            "--date=short",
            &format!("-n{}", limit.max(5)),
            "--name-only",
        ],
    )
    .or_else(|_| {
        git::git_in(
            repo.path(),
            &[
                "log",
                "-G",
                &regex_escape(pat),
                &format!("--pretty=format:{PRETTY_COMMIT}"),
                "--date=short",
                &format!("-n{}", limit.max(5)),
                "--name-only",
            ],
        )
    })?;

    let events: Vec<DigEvent> = git::parse_name_only_log(&out)
        .into_iter()
        .map(|(c, files)| make_event(c, &files))
        .collect();

    // git log is newest-first
    let last = events.first().map(|e| e.commit.clone());
    let first = events.last().map(|e| e.commit.clone());

    let summary = if events.is_empty() {
        format!("no history for `{pat}` — pattern never introduced (or binary-only)")
    } else {
        format!(
            "`{pat}` · {} commits · first {} ({}) · last {} ({})",
            events.len(),
            first.as_ref().map(|c| c.short.as_str()).unwrap_or("?"),
            first.as_ref().map(|c| c.date.as_str()).unwrap_or("?"),
            last.as_ref().map(|c| c.short.as_str()).unwrap_or("?"),
            last.as_ref().map(|c| c.date.as_str()).unwrap_or("?"),
        )
    };

    Ok(ArchaeologyReport {
        pattern: pat.to_string(),
        events,
        first,
        last,
        summary,
    })
}

fn make_event(commit: CommitInfo, files: &[String]) -> DigEvent {
    let subj = commit.subject.to_lowercase();
    let kind = if subj.contains("remove") || subj.contains("delete") || subj.contains("drop") {
        "removed-ish"
    } else if subj.contains("add") || subj.contains("introduc") || subj.contains("initial") {
        "introduced"
    } else {
        "changed"
    };
    let mut files = files.to_vec();
    files.truncate(10);
    DigEvent {
        commit,
        files,
        kind: kind.into(),
    }
}

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if ".+*?^$()[]{}|\\".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
