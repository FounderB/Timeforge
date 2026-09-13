use std::collections::HashMap;

use serde::Serialize;

use crate::git;
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct BlameLine {
    pub line: u32,
    pub hash: String,
    pub author: String,
    pub date: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthorStat {
    pub author: String,
    pub lines: usize,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlameReport {
    pub path: String,
    pub lines: Vec<BlameLine>,
    pub authors: Vec<AuthorStat>,
    pub total_lines: usize,
}

pub fn file_blame(repo: &Repo, path: &str, max_lines: usize) -> Result<BlameReport, String> {
    let rel = git::rel_path(repo.path(), std::path::Path::new(path))
        .unwrap_or_else(|_| path.replace('\\', "/"));

    let out = git::git_in(repo.path(), &["blame", "--line-porcelain", "--", &rel])?;

    let mut lines = Vec::new();
    let mut hash = String::new();
    let mut author = String::new();
    let mut date = String::new();
    let mut line_no = 0u32;
    let mut counts: HashMap<String, usize> = HashMap::new();

    for raw in out.lines() {
        if let Some(stripped) = raw.strip_prefix('\t') {
            line_no += 1;
            let content = stripped.to_string();
            counts
                .entry(author.clone())
                .and_modify(|c| *c += 1)
                .or_insert(1);
            if lines.len() < max_lines {
                lines.push(BlameLine {
                    line: line_no,
                    hash: hash.chars().take(8).collect(),
                    author: author.clone(),
                    date: date.clone(),
                    content,
                });
            }
        } else if let Some(rest) = raw.strip_prefix("author ") {
            author = rest.to_string();
        } else if let Some(rest) = raw.strip_prefix("author-time ") {
            if let Ok(ts) = rest.parse::<i64>() {
                date = chrono::DateTime::from_timestamp(ts, 0)
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| rest.to_string());
            }
        } else if raw.len() >= 40
            && raw
                .as_bytes()
                .iter()
                .take(40)
                .all(|b| b.is_ascii_hexdigit())
        {
            hash = raw.split_whitespace().next().unwrap_or("").to_string();
        }
    }

    let total = counts.values().sum::<usize>().max(1);
    let mut authors: Vec<AuthorStat> = counts
        .into_iter()
        .map(|(author, lines)| AuthorStat {
            percent: (lines as f64) * 100.0 / total as f64,
            author,
            lines,
        })
        .collect();
    authors.sort_by_key(|b| std::cmp::Reverse(b.lines));

    Ok(BlameReport {
        path: rel,
        total_lines: line_no as usize,
        lines,
        authors,
    })
}
