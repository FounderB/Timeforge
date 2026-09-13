use std::time::Instant;

use serde::Serialize;

use crate::archaeology::{dig_pattern_ex, ArchaeologyReport};
use crate::blame_map::{blame_map, BlameMap};
use crate::fixbreak::{fix_break_pairs_ex, FixBreakPair, FixBreakReport};
use crate::pr::{pr_travel, PrTravel};
use crate::query::{self, looks_like_code_token, parse_ask, ParsedAsk};
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
    pub evidence: String,
    pub method: String,
    pub confidence: u32,
    pub drilldowns: Vec<HuntDrilldown>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HuntReport {
    pub query: String,
    pub path: Option<String>,
    pub parsed: ParsedAsk,
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
    let parsed = parse_ask(query, path);
    let q = parsed.raw.clone();
    if q.is_empty() && parsed.path.is_none() {
        return Err("Ask one thing: a keyword, stack line, #PR, or file path".into());
    }

    if let Some(pr_q) = parsed.pr.clone() {
        if is_mostly_pr_ask(&q) || parsed.dig_needle.is_none() {
            match pr_travel(repo, &pr_q, if fast { 6 } else { 12 }) {
                Ok(pr) => return Ok(pr_report(&q, path, parsed, pr, t0)),
                Err(e) if is_mostly_pr_ask(&q) => {
                    let ms = t0.elapsed().as_millis() as u64;
                    return Ok(HuntReport {
                        query: q,
                        path: path.map(|s| s.to_string()),
                        parsed,
                        hits: vec![],
                        answer: None,
                        why: None,
                        dig: None,
                        pairs: None,
                        pr: None,
                        map: None,
                        summary: format!("No commit for #{pr_q} in {ms}ms — {e}"),
                        elapsed_ms: ms,
                        mode: "pr-miss".into(),
                    });
                }
                Err(_) => {}
            }
        }
    }

    let path_owned = parsed.path.clone();
    let bug_like = parsed.bug_like;
    let dig_limit = if fast { 5 } else { 16 };
    let why_limit = if fast { 4 } else { 10 };
    let pair_limit = if fast {
        if bug_like || path_owned.is_some() {
            3
        } else {
            0
        }
    } else if bug_like {
        8
    } else {
        4
    };

    let want_pairs = pair_limit > 0;
    let want_why = !(fast
        && parsed
            .dig_needle
            .as_deref()
            .map(looks_like_code_token)
            .unwrap_or(false)
        && !bug_like);

    let needle_owned = parsed.dig_needle.clone();
    let since_owned = since.to_string();
    let repo_path = repo.path().to_path_buf();
    let why_kw = needle_owned
        .clone()
        .or_else(|| parsed.keywords.first().cloned())
        .filter(|s| !s.is_empty());

    let (why_r, dig_r, pairs_r) = std::thread::scope(|scope| {
        let why_h = scope.spawn(|| {
            if !want_why {
                return None;
            }
            let r = Repo {
                root: repo_path.clone(),
            };
            why_broke(
                &r,
                path_owned.as_deref(),
                &since_owned,
                why_kw.as_deref(),
                why_limit,
            )
            .ok()
        });
        let dig_h = scope.spawn(|| {
            let needle = needle_owned.as_deref()?;
            let r = Repo {
                root: repo_path.clone(),
            };
            dig_pattern_ex(&r, needle, dig_limit, path_owned.as_deref(), fast).ok()
        });
        let pairs_h = scope.spawn(|| {
            if !want_pairs {
                return None;
            }
            let r = Repo {
                root: repo_path.clone(),
            };
            let win = if fast {
                "120 days ago"
            } else {
                since_owned.as_str()
            };
            fix_break_pairs_ex(&r, win, pair_limit, fast).ok()
        });
        (
            why_h.join().ok().flatten(),
            dig_h.join().ok().flatten(),
            pairs_h.join().ok().flatten(),
        )
    });

    let mut hits = Vec::new();

    if let Some(p) = &pairs_r {
        for pair in p.pairs.iter() {
            if pair.method == "cochange" || pair.method == "none" {
                continue;
            }
            if is_noise_fix_subject(&pair.fix.subject) {
                continue;
            }
            if !q.is_empty() && !pair_matches_query(pair, &q) {
                continue;
            }
            let evidence = evidence_for_method(&pair.method);
            let mut score = pair.confidence;
            if pair_matches_query(pair, &q) {
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
            if hits.iter().filter(|h| h.kind == "fix-break").count() >= if fast { 2 } else { 5 } {
                break;
            }
        }
    }

    if let Some(d) = &dig_r {
        let needle = parsed.dig_needle.as_deref().unwrap_or(&q);
        if let Some(first) = &d.first {
            let boost = if looks_like_code_token(needle) {
                90
            } else {
                74
            };
            hits.push(HuntHit {
                kind: "first-seen".into(),
                title: format!(
                    "`{needle}` first seen in {} · {}",
                    first.short, first.subject
                ),
                detail: d.summary.clone(),
                path: d
                    .events
                    .last()
                    .and_then(|e| e.files.first().cloned())
                    .or_else(|| path_owned.clone()),
                commit: Some(first.short.clone()),
                score: boost,
                evidence: if looks_like_code_token(needle) {
                    "proven"
                } else {
                    "strong"
                }
                .into(),
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
                score: if looks_like_code_token(needle) {
                    58
                } else {
                    48
                },
                evidence: "strong".into(),
                method: "dig".into(),
            });
        }
    }

    if let Some(w) = &why_r {
        for s in &w.suspects {
            if why_kw.is_some() && s.score < 18 {
                continue;
            }
            hits.push(HuntHit {
                kind: "suspect".into(),
                title: format!("{} · {}", s.commit.short, s.commit.subject),
                detail: s.reasons.join(" · "),
                path: s.files_touched.first().cloned(),
                commit: Some(s.commit.short.clone()),
                score: s.score.min(62),
                evidence: "heuristic".into(),
                method: "why".into(),
            });
        }
    }

    let map = path_owned.as_deref().and_then(|p| blame_map(repo, p).ok());
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
    hits.truncate(if fast { 8 } else { 16 });

    let answer = compose_answer(
        &q,
        path_owned.as_deref(),
        &hits,
        dig_r.as_ref(),
        pairs_r.as_ref(),
        map.as_ref(),
        parsed.dig_needle.as_deref(),
    );

    let label = if q.is_empty() {
        path_owned.clone().unwrap_or_else(|| "repo".into())
    } else {
        q.clone()
    };
    let ms = t0.elapsed().as_millis() as u64;
    let summary = match (&answer, hits.is_empty()) {
        (None, true) => format!(
            "No strong signal for `{label}` in {ms}ms — try a code token, #PR, or file path"
        ),
        (None, false) => format!(
            "`{label}` · {} weak hits · {ms}ms — no proven answer (evidence too low)",
            hits.len()
        ),
        (Some(a), _) => format!(
            "`{label}` → [{}] {} · {} hits · {ms}ms",
            a.evidence,
            a.headline,
            hits.len(),
        ),
    };

    Ok(HuntReport {
        query: label,
        path: path_owned.or_else(|| path.map(|s| s.to_string())),
        parsed,
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

fn is_mostly_pr_ask(q: &str) -> bool {
    let s = q.trim();
    if s.starts_with('#') {
        return s
            .chars()
            .skip(1)
            .all(|c| c.is_ascii_digit() || c.is_whitespace());
    }
    s.chars().all(|c| c.is_ascii_digit()) && s.len() <= 6
}

fn pr_report(
    q: &str,
    path: Option<&str>,
    parsed: ParsedAsk,
    pr: PrTravel,
    t0: Instant,
) -> HuntReport {
    let path0 = pr.files.first().map(|f| f.path.clone());
    let mut drills = drilldowns_for(path0.as_deref(), Some(q), false, false, path0.is_some());
    drills.insert(
        0,
        HuntDrilldown {
            kind: "timeline".into(),
            label: "After this PR".into(),
            path: path0.clone(),
            query: Some(format!("after {}", pr.pr.as_deref().unwrap_or("#"))),
        },
    );
    let later_n = pr.later.len();
    let answer = HuntAnswer {
        headline: format!(
            "{} landed in {} · {} files · {} later touches",
            pr.pr.as_deref().unwrap_or("#?"),
            pr.commit.short,
            pr.files.len(),
            later_n
        ),
        commit: Some(pr.commit.short.clone()),
        path: path0.clone(),
        why: pr.summary.clone(),
        evidence: "strong".into(),
        method: "pr".into(),
        confidence: 95,
        drilldowns: drills,
    };
    let mut hits = vec![HuntHit {
        kind: "pr".into(),
        title: format!("{} · {}", pr.commit.short, pr.commit.subject),
        detail: pr.summary.clone(),
        path: path0.clone(),
        commit: Some(pr.commit.short.clone()),
        score: 95,
        evidence: "strong".into(),
        method: "pr".into(),
    }];
    for f in pr.files.iter().take(5) {
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
    for l in pr.later.iter().take(4) {
        hits.push(HuntHit {
            kind: "pr-later".into(),
            title: format!("{} · {} · {}", l.commit.short, l.path, l.commit.subject),
            detail: "changed after PR on same path".into(),
            path: Some(l.path.clone()),
            commit: Some(l.commit.short.clone()),
            score: 55,
            evidence: "strong".into(),
            method: "pr-later".into(),
        });
    }
    HuntReport {
        query: q.to_string(),
        path: path.map(|s| s.to_string()),
        parsed,
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
    }
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
    dig_needle: Option<&str>,
) -> Option<HuntAnswer> {
    let mut cands: Vec<(i32, HuntAnswer)> = Vec::new();
    let has_dig = dig.map(|d| !d.events.is_empty()).unwrap_or(false);
    let has_pairs = pairs
        .map(|p| {
            p.pairs
                .iter()
                .any(|x| x.method == "pickaxe" || x.method == "blame")
        })
        .unwrap_or(false);

    if let Some(p) = pairs {
        for pair in &p.pairs {
            if pair.method != "pickaxe" && pair.method != "blame" {
                continue;
            }
            if is_noise_fix_subject(&pair.fix.subject) {
                continue;
            }
            let matched = pair_matches_query(pair, q);
            if pair.method == "blame" && !q.is_empty() && !matched {
                continue;
            }
            if pair.method == "pickaxe" && !q.is_empty() && !matched {
                continue;
            }
            let Some(brk) = &pair.break_commit else {
                continue;
            };
            let path0 = pair.shared_files.first().cloned();
            let mut score = (evidence_rank(evidence_for_method(&pair.method)) as i32) * 100
                + pair.confidence as i32;
            if matched {
                score += 25;
            }
            if pair.method == "pickaxe" {
                score += 18;
            }
            if pair.method == "blame" {
                score -= 8;
            }
            cands.push((
                score,
                HuntAnswer {
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
                        has_dig,
                        true,
                        map.is_some() || path0.is_some(),
                    ),
                },
            ));
        }
    }

    if let Some(d) = dig {
        if let Some(first) = &d.first {
            if let Some(needle) = dig_needle {
                let path0 = d
                    .events
                    .last()
                    .and_then(|e| e.files.first().cloned())
                    .or_else(|| path.map(|s| s.to_string()));
                let code = looks_like_code_token(needle);
                let evidence = if code { "proven" } else { "strong" };
                let conf: u32 = if code { 90 } else { 74 };
                let score = (evidence_rank(evidence) as i32) * 100 + conf as i32;
                cands.push((
                    score,
                    HuntAnswer {
                        headline: format!("`{needle}` first appeared in {}", first.short),
                        commit: Some(first.short.clone()),
                        path: path0.clone(),
                        why: d.summary.clone(),
                        evidence: evidence.into(),
                        method: "dig".into(),
                        confidence: conf,
                        drilldowns: drilldowns_for(
                            path0.as_deref().or(path),
                            Some(q),
                            true,
                            has_pairs,
                            map.is_some() || path0.is_some(),
                        ),
                    },
                ));
            }
        }
    }

    if let Some(top) = hits.first() {
        if !(top.method == "why" && query::is_stopword(q)) {
            let score = (evidence_rank(&top.evidence) as i32) * 100 + top.score as i32 - 30;
            cands.push((
                score,
                HuntAnswer {
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
                        has_dig,
                        has_pairs,
                        map.is_some() || top.path.is_some(),
                    ),
                },
            ));
        }
    }

    cands.sort_by_key(|b| std::cmp::Reverse(b.0));
    cands.into_iter().map(|(_, a)| a).next()
}

fn is_noise_fix_subject(subject: &str) -> bool {
    let s = subject.to_lowercase();
    let noise = [
        "typo",
        "readme",
        "changelog",
        "whitespace",
        "formatting",
        "clippy",
    ];
    if noise.iter().any(|w| s.contains(w)) {
        return ![
            "panic", "deadlock", "crash", "secur", "overflow", "race", "null",
        ]
        .iter()
        .any(|w| s.contains(w));
    }
    if s.starts_with("doc:") || s.starts_with("docs:") {
        return !s.contains("panic") && !s.contains("deadlock");
    }
    false
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
        .filter(|t| t.len() >= 3 && !query::is_stopword(t))
        .collect();
    if tokens.is_empty() {
        return false;
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
        "dig" | "pr" | "blame-map" | "pr-later" => "strong",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::looks_like_bug_ask;

    #[test]
    fn code_token_detection() {
        assert!(looks_like_code_token("GIT_NO_LAZY_FETCH"));
        assert!(looks_like_code_token("unwrap!"));
        assert!(looks_like_bug_ask("auth panic"));
    }
}
