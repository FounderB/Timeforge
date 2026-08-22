use serde::Serialize;

use crate::git;
use crate::Repo;

#[derive(Debug, Clone, Serialize)]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    pub kind: String, // tree | blob
    pub icon: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeListing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<TreeEntry>,
}

pub fn list_tree(repo: &Repo, path: &str) -> Result<TreeListing, String> {
    let path = path.trim_matches('/').to_string();
    let spec = if path.is_empty() {
        "HEAD:".to_string()
    } else {
        format!("HEAD:{path}")
    };

    let out = git::git_in(repo.path(), &["ls-tree", "--name-only", "-z", &spec])
        .or_else(|_| git::git_in(repo.path(), &["ls-tree", "--name-only", &spec]))?;

    let names: Vec<String> = if out.contains('\0') {
        out.split('\0')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    } else {
        out.lines().map(|s| s.to_string()).collect()
    };

    // Get types with full ls-tree
    let typed = git::git_in(repo.path(), &["ls-tree", &spec]).unwrap_or_default();
    let mut kind_map = std::collections::HashMap::new();
    for line in typed.lines() {
        // mode type hash\tname
        let Some((meta, name)) = line.split_once('\t') else {
            continue;
        };
        let parts: Vec<&str> = meta.split_whitespace().collect();
        if parts.len() >= 2 {
            kind_map.insert(name.to_string(), parts[1].to_string());
        }
    }

    let mut entries = Vec::new();
    for name in names {
        let full = if path.is_empty() {
            name.clone()
        } else {
            format!("{path}/{name}")
        };
        let kind = kind_map
            .get(&name)
            .cloned()
            .unwrap_or_else(|| "blob".into());
        let icon = icon_for(&name, &kind);
        entries.push(TreeEntry {
            name,
            path: full,
            kind,
            icon,
        });
    }

    entries.sort_by(|a, b| match (a.kind.as_str(), b.kind.as_str()) {
        ("tree", "blob") => std::cmp::Ordering::Less,
        ("blob", "tree") => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    let parent = if path.is_empty() {
        None
    } else if let Some((p, _)) = path.rsplit_once('/') {
        Some(p.to_string())
    } else {
        Some(String::new())
    };

    Ok(TreeListing {
        path,
        parent,
        entries,
    })
}

fn icon_for(name: &str, kind: &str) -> String {
    if kind == "tree" {
        return "📁".into();
    }
    let lower = name.to_lowercase();
    if lower.ends_with(".rs") {
        "🦀".into()
    } else if lower.ends_with(".ts") || lower.ends_with(".tsx") || lower.ends_with(".js") {
        "📜".into()
    } else if lower.ends_with(".py") {
        "🐍".into()
    } else if lower.ends_with(".go") {
        "🐹".into()
    } else if lower.ends_with(".md") {
        "📝".into()
    } else if lower.ends_with(".json") || lower.ends_with(".toml") || lower.ends_with(".yaml") || lower.ends_with(".yml") {
        "⚙️".into()
    } else if lower.ends_with(".html") || lower.ends_with(".css") {
        "🎨".into()
    } else if lower.ends_with(".svg") || lower.ends_with(".png") || lower.ends_with(".jpg") {
        "🖼️".into()
    } else if lower == "dockerfile" || lower.ends_with(".dockerfile") {
        "🐳".into()
    } else if lower.starts_with("license") {
        "📄".into()
    } else if lower == "cargo.toml" || lower == "package.json" {
        "📦".into()
    } else {
        "📄".into()
    }
}
