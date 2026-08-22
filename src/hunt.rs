use std::time::Instant;

use serde::Serialize;

use crate::archaeology::{dig_pattern, ArchaeologyReport};
use crate::blame_map::{blame_map, BlameMap};
use crate::fixbreak::{fix_break_pairs, FixBreakPair, FixBreakReport};
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
    /// proven | strong | heuristic | weak
    pub evidence: String,
    pub method: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HuntDrilldown {
    pub kind: String,
    pub label: String,
    pub path: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HuntAnswer {
    pub headline: String,
    pub commit: Option<String>,
    pub path: Option<String>,
    pub why: String,
    /// proven | strong | heuristic | weak
    pub evidence: String,
    pub method: String,
    pub confidence: u32,
    pub drilldowns: Vec<HuntDrilldown>,
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
                    evidence: "strong".into(),
                    method: "pr".into(),
                    confidence: 95,
                    drilldowns: drilldowns_for(
                        path0.as_deref(),
                        Some(q),
                        true,
                        false,
                        path0.is_some(),
                    ),
                };
                let mut hits = vec![HuntHit {
                    kind: "pr".into(),
                    title: format!("{} · {}", pr.commit.short, pr.commit.subject),
                    detail: pr.summary.clone(),
                    path: path0,
                    commit: Some(pr.commit.short.clone()),
                    score: 95,
                    evidence: "strong".into(),
                    method: "pr".into(),
                }];
                for f in pr.files.iter().take(6) {
                    hits.push(HuntHit {
                        kind: "pr-file".into(),
                        title: f.path.clone(),
                        detail: format!("+{}/-{}", f.insertions, f.deletions),
                        path: Some(f.path.clone()),
                        commit: Some(pr.commit.short.clone()),
                        score: 60,
                        evidence: "strong".into(),
                        method: "pr".into(),
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
            Err(_) => {}
        }
    }

    let code_like = looks_like_code_token(q);
    let bug_like = looks_like_bug_ask(q);
    let dig_limit = if fast { 8 } else { 16 };
    let why_limit = if fast { 6 } else { 10 };
    let pair_limit = if fast {
        if bug_like { 8 } else { 4 }
    } else {
        10
    };
    let path_owned = path.map(|s| s.to_string());
    let q_owned = q.to_string();
    let since_owned = since.to_string();
    let repo_path = repo.path().to_path_buf();

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
            let win = if fast && !bug_like {
                "180 days ago"
            } else {
                since_owned.as_str()
            };
            fix_break_pairs(&r, win, pair_limit).ok()
        });
        (
            why_h.join().ok().and_then(|r| r.ok()),
            dig_h.join().ok().flatten(),
            pairs_h.join().ok().flatten(),
        )
    });

    let mut hits = Vec::new();

    // —— Fix↔break: only proven/strong into the hit list; filter by query ——
    if let Some(p) = &pairs_r {
        for pair in p.pairs.iter() {
            if pair.method == "cochange" || pair.method == "none" {
                continue;
            }
            if !pair_matches_query(pair, q) && !q.is_empty() && !bug_like {
                continue;
            }
            let evidence = evidence_for_method(&pair.method);
            let mut score = pair.confidence;
            if pair_matches_query(pair, q) {
                score = score.saturating_add(8);
            }
            if bug_like {
                score = score.saturating_add(6);
            }
            hits.push(HuntHit {
                kind: "fix-break".into(),
                title: format!(
                    "{} introduced what {} fixed",
                    pair.break_commit
                        .as_ref()
                        .map(|c| c.short.as_str())
                        .unwrap_or("?"),
                    pair.fix.short
                ),
                detail: format!("{} · {}", pair.method, pair.note),
                path: pair.shared_files.first().cloned(),
                commit: pair
                    .break_commit
                    .as_ref()
                    .map(|c| c.short.clone())
                    .or_else(|| Some(pair.fix.short.clone())),
                score,
                evidence: evidence.into(),
                method: pair.method.clone(),
            });
            hits.push(HuntHit {
                kind: "fix".into(),
                title: format!("{} · {}", pair.fix.short, pair.fix.subject),
                detail: format!("paired via {}", pair.method),
                path: pair.shared_files.first().cloned(),
                commit: Some(pair.fix.short.clone()),
                score: score.saturating_sub(12),
                evidence: evidence.into(),
                method: pair.method.clone(),
            });
            if hits.iter().filter(|h| h.kind == "fix-break").count() >= if fast { 3 } else { 5 } {
                break;
            }
        }
    }

    // —— Dig / archaeology ——
    if let Some(d) = &dig_r {
        if let Some(first) = &d.first {
            let boost = if code_like { 90 } else { 74 };
            hits.push(HuntHit {
                kind: "first-seen".into(),
                title: format!("`{q}` first seen in {} · {}", first.short, first.subject),
                detail: d.summary.clone(),
                path: d.events.last().and_then(|e| e.files.first().cloned()),
                commit: Some(first.short.clone()),
                score: boost,
                evidence: if code_like { "proven" } else { "strong" }.into(),
                method: "dig".into(),
            });
        }
        for e in d.events.iter().take(if fast { 2 } else { 4 }) {
            hits.push(HuntHit {
                kind: format!("dig-{}", e.kind),
                title: format!("{} · {}", e.commit.short, e.commit.subject),
                detail: e.files.join(", "),
                path: e.files.first().cloned(),
                commit: Some(e.commit.short.clone()),
                score: if code_like { 58 } else { 48 },
                evidence: "strong".into(),
                method: "dig".into(),
            });
        }
    }

    // —— Why suspects (heuristic) ——
    if let Some(w) = &why_r {
        for s in &w.suspects {
            if !q.is_empty() && s.score < 18 {
                continue;
            }
            // Cap so keyword heuristics cannot outrank hunk proof
            let score = s.score.min(62);
            hits.push(HuntHit {
                kind: "suspect".into(),
                title: format!("{} · {}", s.commit.short, s.commit.subject),
                detail: s.reasons.join(" · "),
                path: s.files_touched.first().cloned(),
                commit: Some(s.commit.short.clone()),
                score,
                evidence: "heuristic".into(),
                method: "why".into(),
            });
        }
    }

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
            score: 40,
            evidence: "strong".into(),
            method: "blame-map".into(),
        });
    }

    hits.sort_by(|a, b| {
        evidence_rank(&b.evidence)
            .cmp(&evidence_rank(&a.evidence))
            .then(b.score.cmp(&a.score))
    });
    hits.truncate(if fast { 10 } else { 16 });

    let answer = compose_answer(q, path, &hits, dig_r.as_ref(), pairs_r.as_ref(), map.as_ref());

    let label = if q.is_empty() {
        path.unwrap_or("repo").to_string()
    } else {
        q.to_string()
    };
    let ms = t0.elapsed().as_millis() as u64;
    let summary = if hits.is_empty() {
        format!("No strong signal for `{label}` in {ms}ms — try a code token, #PR, or file path")
    } else {
        let ev = answer
            .as_ref()
            .map(|a| a.evidence.as_str())
            .unwrap_or("?");
        format!(
            "`{label}` → [{}] {} · {} hits · {ms}ms",
            ev,
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

fn compose_answer(
    q: &str,
    path: Option<&str>,
    hits: &[HuntHit],
    dig: Option<&ArchaeologyReport>,
    pairs: Option<&FixBreakReport>,
    map: Option<&BlameMap>,
) -> Option<HuntAnswer> {
    // Prefer proven fix↔break (break commit), then dig first-seen, then best hit.
    if let Some(p) = pairs {
        let best = p
            .pairs
            .iter()
            .filter(|x| x.method == "pickaxe" || x.method == "blame")
            .filter(|x| q.is_empty() || looks_like_bug_ask(q) || pair_matches_query(x, q))
            .max_by_key(|x| {
                let mut s = x.confidence as i32;
                if pair_matches_query(x, q) {
                    s += 10;
                }
                if x.method == "pickaxe" {
                    s += 5;
                }
                s
            });
        if let Some(pair) = best {
            if let Some(brk) = &pair.break_commit {
                let path0 = pair.shared_files.first().cloned();
                return Some(HuntAnswer {
                    headline: format!(
                        "{} likely introduced what {} fixed",
                        brk.short, pair.fix.short
                    ),
                    commit: Some(brk.short.clone()),
                    path: path0.clone(),
                    why: format!("{} · {}", pair.method, pair.note),
                    evidence: evidence_for_method(&pair.method).into(),
                    method: pair.method.clone(),
                    confidence: pair.confidence,
                    drilldowns: drilldowns_for(
                        path0.as_deref().or(path),
                        Some(q),
                        dig.map(|d| !d.events.is_empty()).unwrap_or(false),
                        true,
                        map.is_some() || path0.is_some(),
                    ),
                });
            }
        }
    }

    if let Some(d) = dig {
        if let Some(first) = &d.first {
            if looks_like_code_token(q) || q.len() >= 4 {
                let path0 = d.events.last().and_then(|e| e.files.first().cloned());
                return Some(HuntAnswer {
                    headline: format!("`{q}` first appeared in {}", first.short),
                    commit: Some(first.short.clone()),
                    path: path0.clone(),
                    why: d.summary.clone(),
                    evidence: if looks_like_code_token(q) {
                        "proven"
                    } else {
                        "strong"
                    }
                    .into(),
                    method: "dig".into(),
                    confidence: if looks_like_code_token(q) { 90 } else { 74 },
                    drilldowns: drilldowns_for(
                        path0.as_deref().or(path),
                        Some(q),
                        true,
                        pairs.map(|p| p.pairs.iter().any(|x| x.method == "pickaxe")).unwrap_or(false),
                        map.is_some() || path0.is_some(),
                    ),
                });
            }
        }
    }

    let top = hits.first()?;
    Some(HuntAnswer {
        headline: top.title.clone(),
        commit: top.commit.clone(),
        path: top.path.clone(),
        why: if top.detail.is_empty() {
            format!("{} · {}", top.evidence, top.method)
        } else {
            format!("{} · {} · {}", top.evidence, top.method, top.detail)
        },
        evidence: top.evidence.clone(),
        method: top.method.clone(),
        confidence: top.score,
        drilldowns: drilldowns_for(
            top.path.as_deref().or(path),
            Some(q),
            dig.map(|d| !d.events.is_empty()).unwrap_or(false),
            pairs
                .map(|p| {
                    p.pairs
                        .iter()
                        .any(|x| x.method == "pickaxe" || x.method == "blame")
                })
                .unwrap_or(false),
            map.is_some() || top.path.is_some(),
        ),
    })
}

fn drilldowns_for(
    path: Option<&str>,
    query: Option<&str>,
    has_dig: bool,
    has_pairs: bool,
    has_map: bool,
) -> Vec<HuntDrilldown> {
    let mut d = Vec::new();
    if let Some(p) = path.filter(|s| !s.is_empty()) {
        d.push(HuntDrilldown {
            kind: "timeline".into(),
            label: "File history".into(),
            path: Some(p.to_string()),
            query: None,
        });
        if has_map {
            d.push(HuntDrilldown {
                kind: "map".into(),
                label: "Blame map".into(),
                path: Some(p.to_string()),
                query: None,
            });
        }
    }
    if has_dig {
        if let Some(q) = query.filter(|s| !s.is_empty()) {
            d.push(HuntDrilldown {
                kind: "dig".into(),
                label: "When it appeared".into(),
                path: path.map(|s| s.to_string()),
                query: Some(q.to_string()),
            });
        }
    }
    if has_pairs {
        d.push(HuntDrilldown {
            kind: "pairs".into(),
            label: "Fix ↔ break".into(),
            path: path.map(|s| s.to_string()),
            query: query.map(|s| s.to_string()),
        });
    }
    d.truncate(4);
    d
}

fn pair_matches_query(pair: &FixBreakPair, q: &str) -> bool {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    let tokens: Vec<&str> = q
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|t| t.len() >= 3)
        .collect();
    if tokens.is_empty() {
        return pair.shared_files.iter().any(|f| f.to_lowercase().contains(&q));
    }
    let hay = format!(
        "{} {} {} {}",
        pair.fix.subject,
        pair.break_commit
            .as_ref()
            .map(|c| c.subject.as_str())
            .unwrap_or(""),
        pair.shared_files.join(" "),
        pair.note
    )
    .to_lowercase();
    tokens.iter().any(|t| hay.contains(t))
}

fn evidence_for_method(method: &str) -> &'static str {
    match method {
        "pickaxe" => "proven",
        "blame" => "strong",
        "dig" | "pr" | "blame-map" => "strong",
        "cochange" => "weak",
        _ => "heuristic",
    }
}

fn evidence_rank(e: &str) -> u8 {
    match e {
        "proven" => 4,
        "strong" => 3,
        "heuristic" => 2,
        "weak" => 1,
        _ => 0,
    }
}

fn looks_like_code_token(q: &str) -> bool {
    let s = q.trim();
    if s.len() < 2 {
        return false;
    }
    if s.contains("::") || s.contains("->") || s.contains('.') && s.contains('(') {
        return true;
    }
    if s.contains('_') || s.contains('!') {
        return true;
    }
    // camelCase / PascalCase-ish identifier
    let has_lower = s.chars().any(|c| c.is_lowercase());
    let has_upper = s.chars().any(|c| c.is_uppercase());
    if has_lower && has_upper && !s.contains(' ') {
        return true;
    }
    // single token, no spaces, mostly alnum — treat as dig-worthy
    !s.contains(' ')
        && s.chars().filter(|c| c.is_alphanumeric()).count() >= s.len().saturating_mul(3) / 4
        && s.len() >= 4
}

fn looks_like_bug_ask(q: &str) -> bool {
    let s = q.to_lowercase();
    [
        "bug", "fix", "panic", "crash", "regress", "broken", "fail", "error", "unwrap",
        "segfault", "null", "oom", "timeout", "flake",
    ]
    .iter()
    .any(|w| s.contains(w))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_token_detection() {
        assert!(looks_like_code_token("GIT_NO_LAZY_FETCH"));
        assert!(looks_like_code_token("unwrap!"));
        assert!(looks_like_code_token("promisor"));
        assert!(!looks_like_bug_ask("promisor") || looks_like_code_token("promisor"));
        assert!(looks_like_bug_ask("auth panic"));
    }
}
