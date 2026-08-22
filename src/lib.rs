mod activity;
mod blast;
mod blame;
mod git;
mod heatmap;
mod hotspots;
pub mod remote;
pub mod report;
mod stale;
mod timeline;
mod why;
pub mod web;

pub use activity::{commit_churn, contributors};
pub use blast::blast_radius;
pub use blame::file_blame;
pub use heatmap::ownership_heatmap;
pub use hotspots::file_hotspots;
pub use remote::{list_cached, open_github, resolve_repo};
pub use stale::stale_files;
pub use timeline::file_timeline;
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
