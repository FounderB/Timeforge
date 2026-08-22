use serde::Serialize;

use crate::blame::{file_blame, AuthorStat};
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct BlameBlock {
    pub start: u32,
    pub end: u32,
    pub lines: u32,
    pub hash: String,
    pub author: String,
    pub date: String,
    pub preview: String,
    pub color: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlameMap {
    pub path: String,
    pub total_lines: usize,
    pub blocks: Vec<BlameBlock>,
    pub authors: Vec<AuthorStat>,
    pub bus_factor: usize,
    pub summary: String,
}

/// Collapse blame into ownership zones (consecutive lines by same commit).
pub fn blame_map(repo: &Repo, path: &str) -> Result<BlameMap, String> {
    let blame = file_blame(repo, path, usize::MAX)?;
    let mut author_colors: std::collections::HashMap<String, u8> =
        std::collections::HashMap::new();
    let mut next_color = 0u8;

    let mut blocks: Vec<BlameBlock> = Vec::new();
    let mut cur: Option<BlameBlock> = None;

    for line in &blame.lines {
        let color = *author_colors.entry(line.author.clone()).or_insert_with(|| {
            let c = next_color % 8;
            next_color = next_color.wrapping_add(1);
            c
        });

        match cur.as_mut() {
            Some(b) if b.hash == line.hash => {
                b.end = line.line;
                b.lines += 1;
            }
            _ => {
                if let Some(prev) = cur.take() {
                    blocks.push(prev);
                }
                cur = Some(BlameBlock {
                    start: line.line,
                    end: line.line,
                    lines: 1,
                    hash: line.hash.clone(),
                    author: line.author.clone(),
                    date: line.date.clone(),
                    preview: truncate(&line.content, 72),
                    color,
                });
            }
        }
    }
    if let Some(prev) = cur {
        blocks.push(prev);
    }

    // Prefer larger blocks first for the strip visualization order stays line order.
    let top_share = blame.authors.first().map(|a| a.percent).unwrap_or(0.0);
    let bus_factor = if top_share >= 80.0 {
        1
    } else if top_share >= 50.0 {
        2
    } else {
        blame.authors.len().min(5).max(1)
    };

    let summary = if blame.authors.is_empty() {
        "empty file".into()
    } else {
        format!(
            "{} zones · {} authors · bus factor ~{} · top {} ({:.0}%)",
            blocks.len(),
            blame.authors.len(),
            bus_factor,
            blame.authors[0].author,
            blame.authors[0].percent
        )
    };

    Ok(BlameMap {
        path: blame.path,
        total_lines: blame.total_lines,
        blocks,
        authors: blame.authors,
        bus_factor,
        summary,
    })
}

fn truncate(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…")
    } else {
        t
    }
}
