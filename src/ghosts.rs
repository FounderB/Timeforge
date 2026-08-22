use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serde::Serialize;

use crate::git;
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct GhostAuthor {
    pub author: String,
    pub last_commit: String,
    pub days_silent: i64,
    pub owned_files: usize,
    pub sample_paths: Vec<String>,
    pub risk: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathBusFactor {
    pub path: String,
    pub bus_factor: usize,
    pub top_author: String,
    pub top_percent: f64,
    pub authors: usize,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GhostReport {
    pub since_days: i64,
    pub ghosts: Vec<GhostAuthor>,
    pub path_risks: Vec<PathBusFactor>,
    pub summary: String,
}

/// Authors who still "own" paths but went silent — plus per-path bus factor.
pub fn ghost_authors(
    repo: &Repo,
    silent_days: i64,
    path_limit: usize,
) -> Result<GhostReport, String> {
    let since = format!("{silent_days} days ago");

    let recent = git::git_in(
        repo.path(),
        &[
            "log",
            &format!("--since={since}"),
            "--pretty=format:%an",
            "-n",
            "5000",
        ],
    )
    .unwrap_or_default();
    let active: HashSet<String> = recent.lines().map(|s| s.to_string()).collect();

    let hist = git::git_in(
        repo.path(),
        &[
            "log",
            "--pretty=format:@@@%an|%ad",
            "--date=short",
            "--name-only",
            "-n",
            "4000",
        ],
    )?;

    let mut last_date: HashMap<String, String> = HashMap::new();
    let mut files: HashMap<String, HashSet<String>> = HashMap::new();
    let mut path_authors: HashMap<String, HashMap<String, u32>> = HashMap::new();
    let mut current: Option<String> = None;

    for line in hist.lines() {
        if let Some(rest) = line.strip_prefix("@@@") {
            let mut parts = rest.splitn(2, '|');
            let author = parts.next().unwrap_or("").to_string();
            let date = parts.next().unwrap_or("").to_string();
            last_date
                .entry(author.clone())
                .and_modify(|d| {
                    if date > *d {
                        *d = date.clone();
                    }
                })
                .or_insert(date);
            current = Some(author);
        } else if !line.is_empty() {
            if let Some(a) = &current {
                files.entry(a.clone()).or_default().insert(line.to_string());
                *path_authors
                    .entry(line.to_string())
                    .or_default()
                    .entry(a.clone())
                    .or_insert(0) += 1;
            }
        }
    }

    let today = Utc::now().date_naive();
    let mut ghosts = Vec::new();
    for (author, date) in &last_date {
        if active.contains(author) {
            continue;
        }
        let days = parse_days_ago(date, today).unwrap_or(silent_days);
        if days < silent_days {
            continue;
        }
        let owned = files.get(author).map(|s| s.len()).unwrap_or(0);
        if owned == 0 {
            continue;
        }
        let mut sample: Vec<String> = files
            .get(author)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        sample.sort();
        sample.truncate(5);
        let risk = if owned >= 20 || days >= silent_days * 2 {
            "high"
        } else if owned >= 8 {
            "medium"
        } else {
            "watch"
        };
        ghosts.push(GhostAuthor {
            author: author.clone(),
            last_commit: date.clone(),
            days_silent: days,
            owned_files: owned,
            sample_paths: sample,
            risk: risk.into(),
        });
    }
    ghosts.sort_by(|a, b| {
        b.owned_files
            .cmp(&a.owned_files)
            .then(b.days_silent.cmp(&a.days_silent))
    });
    ghosts.truncate(25);

    let mut path_risks = Vec::new();
    for (path, authors) in &path_authors {
        let total: u32 = authors.values().sum::<u32>().max(1);
        let mut ranked: Vec<_> = authors.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1));
        let top = ranked[0];
        let top_percent = (*top.1 as f64) * 100.0 / total as f64;
        if top_percent < 60.0 || ranked.len() > 4 {
            continue;
        }
        let bus = if top_percent >= 85.0 {
            1
        } else if top_percent >= 70.0 {
            2
        } else {
            ranked.len().min(3)
        };
        let warning = if bus <= 2 {
            Some(format!(
                "{} owns {:.0}% of commits touching this path",
                top.0, top_percent
            ))
        } else {
            None
        };
        path_risks.push(PathBusFactor {
            path: path.clone(),
            bus_factor: bus,
            top_author: top.0.clone(),
            top_percent,
            authors: ranked.len(),
            warning,
        });
    }
    path_risks.sort_by(|a, b| {
        a.bus_factor
            .cmp(&b.bus_factor)
            .then(
                b.top_percent
                    .partial_cmp(&a.top_percent)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    path_risks.truncate(path_limit.max(10));

    let summary = format!(
        "{} ghost authors (silent ≥{}d) · {} high bus-factor paths",
        ghosts.len(),
        silent_days,
        path_risks.iter().filter(|p| p.bus_factor <= 2).count()
    );

    Ok(GhostReport {
        since_days: silent_days,
        ghosts,
        path_risks,
        summary,
    })
}

fn parse_days_ago(date: &str, today: chrono::NaiveDate) -> Option<i64> {
    let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    Some((today - d).num_days())
}
