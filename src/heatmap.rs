use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::git;
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct OwnerRow {
    pub author: String,
    pub commits: u32,
    pub files: u32,
    pub risk: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Heatmap {
    pub since: String,
    pub owners: Vec<OwnerRow>,
    pub bus_factor: usize,
    pub warning: Option<String>,
}

pub fn ownership_heatmap(repo: &Repo, since: &str, path: Option<&str>) -> Result<Heatmap, String> {
    let mut args: Vec<String> = vec![
        "log".into(),
        format!("--since={since}"),
        "--pretty=format:@@@%an".into(),
        "--name-only".into(),
    ];
    if let Some(p) = path {
        args.push("--".into());
        args.push(p.into());
    }
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = git::git_in(repo.path(), &refs)?;

    let mut commit_counts: HashMap<String, u32> = HashMap::new();
    let mut file_sets: HashMap<String, HashSet<String>> = HashMap::new();
    let mut current: Option<String> = None;

    for line in out.lines() {
        if let Some(a) = line.strip_prefix("@@@") {
            current = Some(a.to_string());
            *commit_counts.entry(a.to_string()).or_insert(0) += 1;
        } else if !line.is_empty() {
            if let Some(a) = &current {
                file_sets
                    .entry(a.clone())
                    .or_default()
                    .insert(line.to_string());
            }
        }
    }

    let mut owners: Vec<OwnerRow> = commit_counts
        .iter()
        .map(|(author, commits)| {
            let files = file_sets.get(author).map(|s| s.len()).unwrap_or(0) as u32;
            let risk = if *commits >= 15 {
                "high".into()
            } else if *commits >= 5 {
                "medium".into()
            } else {
                "normal".into()
            };
            OwnerRow {
                author: author.clone(),
                commits: *commits,
                files,
                risk,
            }
        })
        .collect();
    owners.sort_by_key(|b| std::cmp::Reverse(b.commits));

    let total: u32 = owners.iter().map(|o| o.commits).sum::<u32>().max(1);
    let top = owners.first().map(|o| o.commits).unwrap_or(0);
    let concentration = top as f64 / total as f64;
    let warning = if concentration > 0.65 && !owners.is_empty() {
        Some(format!(
            "Bus factor risk: {} owns {:.0}% of recent commits",
            owners[0].author,
            concentration * 100.0
        ))
    } else {
        None
    };

    let mut cum = 0u32;
    let mut bf = 0usize;
    for o in &owners {
        cum += o.commits;
        bf += 1;
        if cum as f64 / total as f64 >= 0.5 {
            break;
        }
    }
    if bf == 0 {
        bf = 1;
    }

    Ok(Heatmap {
        since: since.to_string(),
        owners,
        bus_factor: bf,
        warning,
    })
}
