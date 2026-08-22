use std::path::PathBuf;

use clap::{Parser, Subcommand};
use timeforge::{
    blast_radius, file_blame, file_timeline, ownership_heatmap, report, why_broke, Repo,
};

#[derive(Parser)]
#[command(
    name = "timeforge",
    about = "Repo Time Machine — see what changed, who owns it, why it broke",
    version,
    author = "FounderB"
)]
struct Cli {
    /// Repository path (default: cwd)
    #[arg(long, global = true)]
    repo: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Time Machine — commit history for a file
    Timeline {
        path: PathBuf,
        #[arg(long, default_value_t = 30)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Blame+ — ownership of a file
    Blame {
        path: PathBuf,
        #[arg(long, default_value_t = 40)]
        lines: usize,
        #[arg(long)]
        json: bool,
    },
    /// Why did it break? — rank likely culprit commits
    Why {
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long, default_value = "90 days ago")]
        since: String,
        #[arg(long, help = "Keyword: test name, module, error text")]
        query: Option<String>,
        #[arg(long, default_value_t = 8)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Ownership heatmap / bus factor
    Heatmap {
        #[arg(long, default_value = "180 days ago")]
        since: String,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Blast radius — files that move with this path
    Blast {
        path: PathBuf,
        #[arg(long, default_value_t = 15)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Demo web UI
    Serve {
        #[arg(long, default_value = "127.0.0.1:8790")]
        addr: String,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let start = cli
        .repo
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let repo = Repo::discover(&start)?;

    match cli.cmd {
        Commands::Timeline { path, limit, json } => {
            let t = file_timeline(&repo, &path.to_string_lossy(), limit)?;
            if json {
                report::print_json(&t);
            } else {
                report::print_timeline(&t);
            }
        }
        Commands::Blame { path, lines, json } => {
            let b = file_blame(&repo, &path.to_string_lossy(), lines)?;
            if json {
                report::print_json(&b);
            } else {
                report::print_blame(&b);
            }
        }
        Commands::Why {
            path,
            since,
            query,
            limit,
            json,
        } => {
            let p = path.as_ref().map(|p| p.to_string_lossy().to_string());
            let w = why_broke(&repo, p.as_deref(), &since, query.as_deref(), limit)?;
            if json {
                report::print_json(&w);
            } else {
                report::print_why(&w);
            }
        }
        Commands::Heatmap { since, path, json } => {
            let p = path.as_ref().map(|p| p.to_string_lossy().to_string());
            let h = ownership_heatmap(&repo, &since, p.as_deref())?;
            if json {
                report::print_json(&h);
            } else {
                report::print_heatmap(&h);
            }
        }
        Commands::Blast { path, limit, json } => {
            let b = blast_radius(&repo, &path.to_string_lossy(), limit)?;
            if json {
                report::print_json(&b);
            } else {
                report::print_blast(&b);
            }
        }
        Commands::Serve { addr } => {
            timeforge::web::serve(&addr, repo.path())?;
        }
    }
    Ok(())
}
