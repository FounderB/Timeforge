use serde::Serialize;

use crate::why::co_changed_files;
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct BlastHit {
    pub path: String,
    pub co_changes: u32,
    pub affinity: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlastReport {
    pub path: String,
    pub related: Vec<BlastHit>,
    pub summary: String,
}

pub fn blast_radius(repo: &Repo, path: &str, limit: usize) -> Result<BlastReport, String> {
    let counts = co_changed_files(repo, path, limit)?;
    let max = counts.values().copied().max().unwrap_or(1).max(1);
    let mut related: Vec<BlastHit> = counts
        .into_iter()
        .map(|(path, co_changes)| BlastHit {
            affinity: co_changes as f64 / max as f64,
            path,
            co_changes,
        })
        .collect();
    related.sort_by(|a, b| b.co_changes.cmp(&a.co_changes));

    let summary = if related.is_empty() {
        "No co-change history — new or isolated file.".into()
    } else {
        format!(
            "Touching this file historically moves {} other path(s). Review those too.",
            related.len()
        )
    };

    Ok(BlastReport {
        path: path.to_string(),
        related,
        summary,
    })
}
