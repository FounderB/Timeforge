mod activity;
mod archaeology;
mod blame;
mod blame_map;
mod blast;
mod fixbreak;
mod ghosts;
mod git;
mod heatmap;
mod hotspots;
mod hunt;
mod pr;
pub mod remote;
pub mod report;
mod stale;
mod timeline;
mod tree;
mod why;
pub mod web;

pub use activity::{commit_churn, contributors};
pub use archaeology::dig_pattern;
pub use blame::file_blame;
pub use blame_map::blame_map;
pub use blast::blast_radius;
pub use fixbreak::fix_break_pairs;
pub use ghosts::ghost_authors;
pub use heatmap::ownership_heatmap;
pub use hotspots::file_hotspots;
pub use hunt::{bug_hunt, regression_radar};
pub use pr::pr_travel;
pub use remote::{list_cached, open_github, repair_cache, resolve_repo, update_repo};
pub use stale::stale_files;
pub use timeline::file_timeline;
pub use tree::{list_tree, list_tree_ex};
pub use why::why_broke;

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Repo {
    pub root: PathBuf,
}

impl Repo {
    pub fn discover(start: &Path) -> Result<Self, String> {
        let root = git::rev_parse_show_toplevel(start)?;
        Ok(Self { root })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }
}
