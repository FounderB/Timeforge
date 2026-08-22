use std::time::Instant;

use serde::Serialize;

use crate::archaeology::{dig_pattern, ArchaeologyReport};
use crate::blame_map::{blame_map, BlameMap};
use crate::fixbreak::{fix_break_pairs, FixBreakReport};
use crate::pr::{pr_travel, PrTravel};
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
pub struct HuntAnswer {
    pub headline: String,
    pub commit: Option<String>,
    pub path: Option<String>,
    pub why: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HuntReport {
    pub query: String,
    pub path: Option<String>,
    pub hits: Vec<HuntHit>,
    pub answer: Option<HuntAnswer>,
    pub why: Option<WhyReport>,
    pub dig: Option<ArchaeologyReport>,
    pub pairs: Option<FixBreakReport>,
    pub pr: Option<PrTravel>,
    pub map: Option<BlameMap>,
    pub summary: String,
    pub elapsed_ms: u64,
    pub mode: String,
}

/// Fast one-question hunt. Parallel light probes; skips heavy ghosts/full blame by default.
pub fn bug_hunt(
    repo: &Repo,
    query: &str,
    path: Option<&str>,
    since: &str,
) -> Result<HuntReport, String> {
    bug_hunt_ex(repo, query, path, since, true)
}

pub fn bug_hunt_ex(
    repo: &Repo,
    query: &str,
    path: Option<&str>,
    since: &str,
    fast: bool,
) -> Result<HuntReport, String> {
    let t0 = Instant::now();
    let q = query.trim();
    if q.is_empty() && path.is_none() {
        return Err("Ask one thing: a keyword, stack line, #PR, or file path".into());
    }

    // Route: bare PR number → PR travel (fastest answer for that question)
    if let Some(pr_q) = looks_like_pr(q) {
        match pr_travel(repo, &pr_q, if fast { 8 } else { 12 }) {
            Ok(pr) => {
                let path0 = pr.files.first().map(|f| f.path.clone());
                let answer = HuntAnswer {
                    headline: format!(
                        "{} landed in {} · {} files",
                        pr.pr.as_deref().unwrap_or("#?"),
                        pr.commit.short,
                        pr.files.len()
                    ),
                    commit: Some(pr.commit.short.clone()),
                    path: path0.clone(),
                    why: pr.summary.clone(),
                };
                let mut hits = vec![HuntHit {
                    kind: "pr".into(),
                    title: format!("{} · {}", pr.commit.short, pr.commit.subject),
                    detail: pr.summary.clone(),
                    path: path0,
                    commit: Some(pr.commit.short.clone()),
                    score: 95,
                }];
                for f in pr.files.iter().take(6) {
                    hits.push(HuntHit {
                        kind: "pr-file".into(),
                        title: f.path.clone(),
                        detail: format!("+{}/-{}", f.insertions, f.deletions),
                        path: Some(f.path.clone()),
                        commit: Some(pr.commit.short.clone()),
                        score: 60,
                    });
                }
                return Ok(HuntReport {
                    query: q.to_string(),
                    path: path.map(|s| s.to_string()),
                    hits,
                    answer: Some(answer),
                    why: None,
                    dig: None,
                    pairs: None,
                    pr: Some(pr),
                    map: None,
                    summary: format!("PR answer in {}ms", t0.elapsed().as_millis()),
                    elapsed_ms: t0.elapsed().as_millis() as u64,
                    mode: "pr-fast".into(),
                });
            }
            Err(_) => {
                // Fall through to keyword hunt (e.g. "#12" not in this repo)
            }
        }
    }

    let dig_limit = if fast { 8 } else { 16 };
    let why_limit = if fast { 6 } else { 10 };
    let pair_limit = if fast { 4 } else { 8 };
    let path_owned = path.map(|s| s.to_string());
    let q_owned = q.to_string();
    let since_owned = since.to_string();
    let repo_path = repo.path().to_path_buf();

    // Parallel: why + dig + pairs (independent git processes)
    let (why_r, dig_r, pairs_r) = std::thread::scope(|scope| {
        let why_h = scope.spawn(|| {
            let r = Repo {
                root: repo_path.clone(),
            };
            why_broke(
                &r,
                path_owned.as_deref(),
                &since_owned,
                Some(q_owned.as_str()).filter(|s| !s.is_empty()),
                why_limit,
            )
        });
        let dig_h = scope.spawn(|| {
            if q_owned.len() < 2 {
                return None;
            }
            let r = Repo {
                root: repo_path.clone(),
            };
            dig_pattern(&r, &q_owned, dig_limit).ok()
        });
        let pairs_h = scope.spawn(|| {
            let r = Repo {
                root: repo_path.clone(),
            };
            // Shorter window when fast
            let win = if fast { "180 days ago" } else { since_owned.as_str() };
            fix_break_pairs(&r, win, pair_limit).ok()
        });
        (
            why_h.join().ok().and_then(|r| r.ok()),
            dig_h.join().ok().flatten(),
            pairs_h.join().ok().flatten(),
        )
    });

    let mut hits = Vec::new();

    if let Some(w) = &why_r {
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

    if let Some(d) = &dig_r {
        if let Some(first) = &d.first {
            hits.push(HuntHit {
                kind: "first-seen".into(),
                title: format!("first seen in {} · {}", first.short, first.subject),
                detail: d.summary.clone(),
                path: d.events.last().and_then(|e| e.files.first().cloned()),
                commit: Some(first.short.clone()),
                score: 72,
            });
        }
        for e in d.events.iter().take(if fast { 3 } else { 5 }) {
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

    if let Some(p) = &pairs_r {
        for pair in p.pairs.iter().take(if fast { 3 } else { 5 }) {
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

    // Blame only when path given (slow on large files)
    let map = if let Some(p) = path {
        blame_map(repo, p).ok()
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
    hits.truncate(if fast { 12 } else { 20 });

    let answer = hits.first().map(|h| HuntAnswer {
        headline: h.title.clone(),
        commit: h.commit.clone(),
        path: h.path.clone(),
        why: if h.detail.is_empty() {
            h.kind.clone()
        } else {
            format!("{} · {}", h.kind, h.detail)
        },
    });

    let label = if q.is_empty() {
        path.unwrap_or("repo").to_string()
    } else {
        q.to_string()
    };
    let ms = t0.elapsed().as_millis() as u64;
    let summary = if hits.is_empty() {
        format!("No strong signal for `{label}` in {ms}ms — try a code token, #PR, or file path")
    } else {
        format!(
            "`{label}` → {} · {} hits · {ms}ms",
            answer
                .as_ref()
                .map(|a| a.headline.as_str())
                .unwrap_or("?"),
            hits.len(),
        )
    };

    Ok(HuntReport {
        query: label,
        path: path.map(|s| s.to_string()),
        hits,
        answer,
        why: why_r,
        dig: dig_r,
        pairs: pairs_r,
        pr: None,
        map,
        summary,
        elapsed_ms: ms,
        mode: if fast { "fast".into() } else { "full".into() },
    })
}

pub fn regression_radar(
    repo: &Repo,
    query: &str,
    path: Option<&str>,
    since: &str,
) -> Result<HuntReport, String> {
    bug_hunt(repo, query, path, since)
}

fn looks_like_pr(q: &str) -> Option<String> {
    let s = q.trim();
    if s.is_empty() {
        return None;
    }
    if s.starts_with('#') {
        let n: String = s.chars().skip(1).take_while(|c| c.is_ascii_digit()).collect();
        if !n.is_empty() && s.chars().skip(1 + n.len()).all(|c| c.is_whitespace()) {
            return Some(n);
        }
    }
    if s.chars().all(|c| c.is_ascii_digit()) && s.len() <= 6 {
        return Some(s.to_string());
    }
    if let Some(rest) = s.strip_prefix("pull/") {
        let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !n.is_empty() {
            return Some(n);
        }
    }
    None
}
