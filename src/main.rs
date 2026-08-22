use std::path::PathBuf;

use clap::{Parser, Subcommand};
use timeforge::{
    blame_map, blast_radius, bug_hunt, commit_churn, contributors, dig_pattern, file_blame,
    file_hotspots, file_timeline, fix_break_pairs, ghost_authors, list_cached, list_tree,
    open_github, ownership_heatmap, pr_travel, remote, repair_cache, report, resolve_repo,
    stale_files, update_repo, why_broke,
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
        spec: String,
        #[arg(long, help = "git fetch even if already cached")]
        update: bool,
        #[arg(
            long,
            alias = "full",
            help = "delete broken partial/promisor cache and re-clone fully"
        )]
        repair: bool,
    },
    /// Fetch + fast-forward current repo to latest remote
    Update {
        #[arg(long)]
        json: bool,
    },
    /// List cached remote repositories (~/.timeforge/repos)
    Repos {
        #[arg(long)]
        json: bool,
    },
    /// Time Machine — commit history for a file (rename-aware)
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
    /// Blame Map — ownership zones across a file
    Map {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// PR Time Travel — files + later churn for #N
    Pr {
        query: String,
        #[arg(long, default_value_t = 12)]
        later: usize,
        #[arg(long)]
        json: bool,
    },
    /// Ghost authors + path bus-factor risks
    Ghosts {
        #[arg(long, default_value_t = 180)]
        days: i64,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Bug archaeology — when a code pattern first appeared (git pickaxe)
    Dig {
        pattern: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Fix ↔ break pairs
    Pairs {
        #[arg(long, default_value = "365 days ago")]
        since: String,
        #[arg(long, default_value_t = 12)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Bug hunt / regression radar — combined suspects + dig + pairs
    Hunt {
        #[arg(long, help = "keyword, stack fragment, test name")]
        query: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long, default_value = "180 days ago")]
        since: String,
        #[arg(long)]
        json: bool,
    },
    /// Alias for hunt
    Radar {
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long, default_value = "180 days ago")]
        since: String,
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
    Heatmap {
        #[arg(long, default_value = "180 days ago")]
        since: String,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    Blast {
        path: PathBuf,
        #[arg(long, default_value_t = 15)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    Hotspots {
        #[arg(long, default_value = "180 days ago")]
        since: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    Stale {
        #[arg(long, default_value_t = 180)]
        days: i64,
        #[arg(long, default_value_t = 30)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    Contributors {
        #[arg(long, default_value = "365 days ago")]
        since: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    Churn {
        #[arg(long, default_value = "365 days ago")]
        since: String,
        #[arg(long)]
        json: bool,
    },
    Tree {
        #[arg(default_value = "")]
        path: String,
        #[arg(long)]
        json: bool,
    },
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
        Commands::Open {
            spec,
            update,
            repair,
        } => {
            let (owner, name) = remote::parse_github_spec(spec)?;
            let repo = if *repair {
                repair_cache(&owner, &name)?
            } else {
                open_github(&owner, &name, *update || cli.update, false)?
            };
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

    let spec = cli.repo.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".into())
    });

    let repo = if remote::is_remote_spec(&spec) {
        let (owner, name) = remote::parse_github_spec(&spec)?;
        open_github(&owner, &name, cli.update, false)?
    } else {
        resolve_repo(&spec)?
    };

    match cli.cmd {
        Commands::Open { .. } | Commands::Repos { .. } => unreachable!(),
        Commands::Update { json } => {
            let u = update_repo(&repo)?;
            if json {
                report::print_json(&u);
            } else {
                println!("update  {}", u.message);
                println!("HEAD    {} → {}", u.before, u.after);
            }
        }
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
        Commands::Map { path, json } => {
            let m = blame_map(&repo, &path.to_string_lossy())?;
            if json {
                report::print_json(&m);
            } else {
                report::print_blame_map(&m);
            }
        }
        Commands::Pr { query, later, json } => {
            let p = pr_travel(&repo, &query, later)?;
            if json {
                report::print_json(&p);
            } else {
                report::print_pr(&p);
            }
        }
        Commands::Ghosts { days, limit, json } => {
            let g = ghost_authors(&repo, days, limit)?;
            if json {
                report::print_json(&g);
            } else {
                report::print_ghosts(&g);
            }
        }
        Commands::Dig {
            pattern,
            limit,
            json,
        } => {
            let d = dig_pattern(&repo, &pattern, limit)?;
            if json {
                report::print_json(&d);
            } else {
                report::print_dig(&d);
            }
        }
        Commands::Pairs {
            since,
            limit,
            json,
        } => {
            let p = fix_break_pairs(&repo, &since, limit)?;
            if json {
                report::print_json(&p);
            } else {
                report::print_pairs(&p);
            }
        }
        Commands::Hunt {
            query,
            path,
            since,
            json,
        }
        | Commands::Radar {
            query,
            path,
            since,
            json,
        } => {
            let p = path.as_ref().map(|p| p.to_string_lossy().to_string());
            let q = query.unwrap_or_default();
            let h = bug_hunt(&repo, &q, p.as_deref(), &since)?;
            if json {
                report::print_json(&h);
            } else {
                report::print_hunt(&h);
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
        Commands::Tree { path, json } => {
            let t = list_tree(&repo, &path)?;
            if json {
                report::print_json(&t);
            } else {
                let here = if t.path.is_empty() {
                    "/"
                } else {
                    t.path.as_str()
                };
                println!("tree  {here}");
                for e in &t.entries {
                    let mark = if e.kind == "tree" { "/" } else { "" };
                    println!("  {} {}{}", e.icon, e.name, mark);
                }
            }
        }
        Commands::Serve { addr } => {
            timeforge::web::serve(&addr, repo.path())?;
        }
    }
    Ok(())
}
