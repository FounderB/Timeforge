mod blast;
mod blame;
mod git;
mod heatmap;
pub mod report;
mod timeline;
mod why;
pub mod web;

pub use blast::blast_radius;
pub use blame::file_blame;
pub use heatmap::ownership_heatmap;
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
