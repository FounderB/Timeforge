use std::path::PathBuf;

use clap::{Parser, Subcommand};
use timeforge::{
    blast_radius, commit_churn, contributors, file_blame, file_hotspots, file_timeline,
    list_cached, open_github, ownership_heatmap, remote, report, resolve_repo, stale_files,
    why_broke,
};

#[derive(Parser)]
#[command(
    name = "timeforge",
    about = "Repo Time Machine — local or any GitHub repo",
    version,
    author = "FounderB"
)]
struct Cli {
    /// Local path OR GitHub spec: owner/repo | https://github.com/owner/repo
    #[arg(long, global = true, env = "TIMEFORGE_REPO")]
    repo: Option<String>,

    /// Fetch latest for cached remote repos
    #[arg(long, global = true)]
    update: bool,

    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Open / cache a GitHub repo (owner/repo or URL)
    Open {
        /// e.g. FounderB/SignShield or https://github.com/rust-lang/mdBook
        spec: String,
        #[arg(long, help = "git fetch even if already cached")]
        update: bool,
    },
    /// List cached remote repositories (~/.timeforge/repos)
    Repos {
        #[arg(long)]
        json: bool,
    },
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
    /// Files with highest change churn
    Hotspots {
        #[arg(long, default_value = "180 days ago")]
        since: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Files not touched for N days
    Stale {
        #[arg(long, default_value_t = 180)]
        days: i64,
        #[arg(long, default_value_t = 30)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Top contributors
    Contributors {
        #[arg(long, default_value = "365 days ago")]
        since: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Weekly commit churn chart
    Churn {
        #[arg(long, default_value = "365 days ago")]
        since: String,
        #[arg(long)]
        json: bool,
    },
    /// Demo web UI (supports opening remotes from the UI)
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

    match &cli.cmd {
        Commands::Open { spec, update } => {
            let (owner, name) = remote::parse_github_spec(spec)?;
            let repo = open_github(&owner, &name, *update || cli.update)?;
            println!("opened {}/{}", owner, name);
            println!("path  {}", repo.path().display());
            println!("tip   timeforge --repo {}/{} timeline README.md", owner, name);
            return Ok(());
        }
        Commands::Repos { json } => {
            let list = list_cached()?;
            if *json {
                report::print_json(&list);
            } else {
                report::print_cached(&list);
            }
            return Ok(());
        }
        _ => {}
    }

    let spec = cli
        .repo
        .clone()
        .unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".into())
        });

    let repo = if remote::is_remote_spec(&spec) {
        let (owner, name) = remote::parse_github_spec(&spec)?;
        open_github(&owner, &name, cli.update)?
    } else {
        resolve_repo(&spec)?
    };

    match cli.cmd {
        Commands::Open { .. } | Commands::Repos { .. } => unreachable!(),
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
        Commands::Hotspots { since, limit, json } => {
            let h = file_hotspots(&repo, &since, limit)?;
            if json {
                report::print_json(&h);
            } else {
                report::print_hotspots(&h);
            }
        }
        Commands::Stale { days, limit, json } => {
            let s = stale_files(&repo, days, limit)?;
            if json {
                report::print_json(&s);
            } else {
                report::print_stale(&s);
            }
        }
        Commands::Contributors { since, limit, json } => {
            let c = contributors(&repo, &since, limit)?;
            if json {
                report::print_json(&c);
            } else {
                report::print_contributors(&c);
            }
        }
        Commands::Churn { since, json } => {
            let c = commit_churn(&repo, &since)?;
            if json {
                report::print_json(&c);
            } else {
                report::print_churn(&c);
            }
        }
        Commands::Serve { addr } => {
            timeforge::web::serve(&addr, repo.path())?;
        }
    }
    Ok(())
}
