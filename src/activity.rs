use std::collections::HashMap;

use serde::Serialize;

use crate::git;
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct Contributor {
    pub author: String,
    pub email: String,
    pub commits: u32,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContributorsReport {
    pub since: String,
    pub total_commits: u32,
    pub contributors: Vec<Contributor>,
}

pub fn contributors(repo: &Repo, since: &str, limit: usize) -> Result<ContributorsReport, String> {
    let out = git::git_in(
        repo.path(),
        &[
            "log",
            &format!("--since={since}"),
            "--pretty=format:%an|%ae",
        ],
    )?;

    let mut map: HashMap<(String, String), u32> = HashMap::new();
    for line in out.lines() {
        let mut p = line.splitn(2, '|');
        let an = p.next().unwrap_or("?").to_string();
        let ae = p.next().unwrap_or("").to_string();
        *map.entry((an, ae)).or_insert(0) += 1;
    }
    let total: u32 = map.values().sum::<u32>().max(1);
    let mut contributors: Vec<Contributor> = map
        .into_iter()
        .map(|((author, email), commits)| Contributor {
            percent: commits as f64 * 100.0 / total as f64,
            author,
            email,
            commits,
        })
        .collect();
    contributors.sort_by(|a, b| b.commits.cmp(&a.commits));
    contributors.truncate(limit);

    Ok(ContributorsReport {
        since: since.to_string(),
        total_commits: total,
        contributors,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ChurnBucket {
    pub week: String,
    pub commits: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChurnReport {
    pub since: String,
    pub buckets: Vec<ChurnBucket>,
}

pub fn commit_churn(repo: &Repo, since: &str) -> Result<ChurnReport, String> {
    let out = git::git_in(
        repo.path(),
        &[
            "log",
            &format!("--since={since}"),
            "--pretty=format:%ad",
            "--date=format:%Y-%W",
        ],
    )?;
    let mut map: HashMap<String, u32> = HashMap::new();
    for week in out.lines() {
        if !week.is_empty() {
            *map.entry(week.to_string()).or_insert(0) += 1;
        }
    }
    let mut buckets: Vec<ChurnBucket> = map
        .into_iter()
        .map(|(week, commits)| ChurnBucket { week, commits })
        .collect();
    buckets.sort_by(|a, b| a.week.cmp(&b.week));
    Ok(ChurnReport {
        since: since.to_string(),
        buckets,
    })
}
