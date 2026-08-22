use serde::Serialize;

use crate::archaeology::{dig_pattern, ArchaeologyReport};
use crate::blame_map::{blame_map, BlameMap};
use crate::fixbreak::{fix_break_pairs, FixBreakReport};
use crate::ghosts::{ghost_authors, GhostReport};
use crate::why::{why_broke, WhyReport};
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct HuntHit {
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub path: Option<String>,
    pub commit: Option<String>,
    pub score: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct HuntReport {
    pub query: String,
    pub path: Option<String>,
    pub hits: Vec<HuntHit>,
    pub why: Option<WhyReport>,
    pub dig: Option<ArchaeologyReport>,
    pub pairs: Option<FixBreakReport>,
    pub ghosts: Option<GhostReport>,
    pub map: Option<BlameMap>,
    pub summary: String,
}

/// One-shot bug hunt: suspects + archaeology + fix/break + optional blame map.
pub fn bug_hunt(
    repo: &Repo,
    query: &str,
    path: Option<&str>,
    since: &str,
) -> Result<HuntReport, String> {
    let q = query.trim();
    if q.is_empty() && path.is_none() {
        return Err("provide a keyword, stack fragment, or file path".into());
    }

    let mut hits = Vec::new();

    let why = why_broke(repo, path, since, Some(q).filter(|s| !s.is_empty()), 8).ok();
    if let Some(w) = &why {
        for s in &w.suspects {
            if !q.is_empty() && s.score < 18 {
                continue;
            }
            hits.push(HuntHit {
                kind: "suspect".into(),
                title: format!("{} · {}", s.commit.short, s.commit.subject),
                detail: s.reasons.join(" · "),
                path: s.files_touched.first().cloned(),
                commit: Some(s.commit.short.clone()),
                score: s.score,
            });
        }
    }

    let dig = if q.len() >= 2 {
        dig_pattern(repo, q, 12).ok()
    } else {
        None
    };
    if let Some(d) = &dig {
        if let Some(first) = &d.first {
            hits.push(HuntHit {
                kind: "first-seen".into(),
                title: format!("first seen in {} · {}", first.short, first.subject),
                detail: d.summary.clone(),
                path: d.events.last().and_then(|e| e.files.first().cloned()),
                commit: Some(first.short.clone()),
                score: 70,
            });
        }
        for e in d.events.iter().take(4) {
            hits.push(HuntHit {
                kind: format!("dig-{}", e.kind),
                title: format!("{} · {}", e.commit.short, e.commit.subject),
                detail: e.files.join(", "),
                path: e.files.first().cloned(),
                commit: Some(e.commit.short.clone()),
                score: 55,
            });
        }
    }

    let pairs = fix_break_pairs(repo, since, 8).ok();
    if let Some(p) = &pairs {
        for pair in p.pairs.iter().take(5) {
            let br = pair
                .break_commit
                .as_ref()
                .map(|c| format!("{} {}", c.short, c.subject))
                .unwrap_or_else(|| "unknown break".into());
            hits.push(HuntHit {
                kind: "fix-break".into(),
                title: format!("fix {} ↔ {}", pair.fix.short, br),
                detail: pair.note.clone(),
                path: pair.shared_files.first().cloned(),
                commit: Some(pair.fix.short.clone()),
                score: pair.confidence,
            });
        }
    }

    let ghosts = ghost_authors(repo, 180, 8).ok();
    if let Some(g) = &ghosts {
        for a in g.ghosts.iter().take(3) {
            hits.push(HuntHit {
                kind: "ghost".into(),
                title: format!("ghost {} · silent {}d", a.author, a.days_silent),
                detail: a.sample_paths.join(", "),
                path: a.sample_paths.first().cloned(),
                commit: None,
                score: if a.risk == "high" { 60 } else { 40 },
            });
        }
    }

    let map = if let Some(p) = path {
        blame_map(repo, p).ok()
    } else if dig.as_ref().map(|d| !d.events.is_empty()).unwrap_or(false) {
        hits.iter()
            .find_map(|h| h.path.clone())
            .and_then(|p| blame_map(repo, &p).ok())
    } else {
        None
    };
    if let Some(m) = &map {
        hits.push(HuntHit {
            kind: "blame-map".into(),
            title: m.summary.clone(),
            detail: format!("bus factor ~{}", m.bus_factor),
            path: Some(m.path.clone()),
            commit: None,
            score: 45,
        });
    }

    hits.sort_by(|a, b| b.score.cmp(&a.score));
    hits.truncate(24);

    let label = if q.is_empty() {
        path.unwrap_or("repo").to_string()
    } else {
        q.to_string()
    };
    let summary = if hits.is_empty() {
        format!("Bug hunt `{label}` · no strong signal — try a real code token or file path")
    } else {
        format!(
            "Bug hunt `{label}` · {} hits · {} suspects · dig {} · {} fix pairs",
            hits.len(),
            why.as_ref().map(|w| w.suspects.len()).unwrap_or(0),
            dig.as_ref().map(|d| d.events.len()).unwrap_or(0),
            pairs.as_ref().map(|p| p.pairs.len()).unwrap_or(0),
        )
    };

    Ok(HuntReport {
        query: label,
        path: path.map(|s| s.to_string()),
        hits,
        why,
        dig,
        pairs,
        ghosts,
        map,
        summary,
    })
}

/// Alias focused on regression wording.
pub fn regression_radar(
    repo: &Repo,
    query: &str,
    path: Option<&str>,
    since: &str,
) -> Result<HuntReport, String> {
    bug_hunt(repo, query, path, since)
}
