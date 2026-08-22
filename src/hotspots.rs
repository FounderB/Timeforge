use std::collections::HashMap;

use serde::Serialize;

use crate::git;
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct Hotspot {
    pub path: String,
    pub commits: u32,
    pub authors: u32,
    pub score: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct HotspotsReport {
    pub since: String,
    pub hotspots: Vec<Hotspot>,
}

pub fn file_hotspots(repo: &Repo, since: &str, limit: usize) -> Result<HotspotsReport, String> {
    let out = git::git_in(
        repo.path(),
        &[
            "log",
            &format!("--since={since}"),
            "--pretty=format:@@@%an",
            "--name-only",
        ],
    )?;

    let mut commits: HashMap<String, u32> = HashMap::new();
    let mut authors: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
    let mut current_author = String::new();

    for line in out.lines() {
        if let Some(a) = line.strip_prefix("@@@") {
            current_author = a.to_string();
        } else if !line.is_empty() {
            *commits.entry(line.to_string()).or_insert(0) += 1;
            authors
                .entry(line.to_string())
                .or_default()
                .insert(current_author.clone());
        }
    }

    let mut hotspots: Vec<Hotspot> = commits
        .into_iter()
        .map(|(path, commits)| {
            let author_n = authors.get(&path).map(|s| s.len()).unwrap_or(1) as u32;
            let score = commits * 2 + author_n * 3;
            Hotspot {
                path,
                commits,
                authors: author_n,
                score,
            }
        })
        .collect();
    hotspots.sort_by(|a, b| b.score.cmp(&a.score));
    hotspots.truncate(limit);

    Ok(HotspotsReport {
        since: since.to_string(),
        hotspots,
    })
}
