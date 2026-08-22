use std::path::PathBuf;

use clap::{Parser, Subcommand};
use timeforge::{
    blame_map, blast_radius, bug_hunt, clean_cached, commit_churn, contributors, dig_pattern,
    file_blame, file_hotspots, file_timeline, fix_break_pairs, ghost_authors, list_cached,
    list_tree, open_github, ownership_heatmap, pr_travel, remote, remove_cached, repair_cache,
    repair_current, report, resolve_repo, stale_files, update_repo, why_broke,
};

#[derive(Parser)]
#[command(
    name = "timeforge",
    about = "Repo Time Machine — ask what broke, who owns it, when it changed",
    after_help = "Ask first:\n  timeforge ask \"auth panic\"\n  timeforge \"#42\"\n  timeforge ask --path src/auth.rs login\n\nHidden power tools still work: timeline, blame, dig, pairs, why, …",
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

    /// Bare ask without subcommand: timeforge auth panic
    #[arg(num_args = 0..)]
    query: Vec<String>,

    #[command(subcommand)]
    cmd: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Ask one thing — keyword, stack line, #PR, or path
    Ask {
        /// Free-form question tokens (flags like --json may follow)
        #[arg(num_args = 0..)]
        query: Vec<String>,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long, default_value = "180 days ago")]
        since: String,
        #[arg(long)]
        json: bool,
    },
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
    /// Fetch + fast-forward / materialize current repo
    Update {
        #[arg(long)]
        json: bool,
    },
    /// Repair current clone (materialize or full re-clone from origin)
    Repair {
        #[arg(long)]
        json: bool,
    },
    /// List / remove cached remote repositories (~/.timeforge/repos)
    Repos {
        #[command(subcommand)]
        action: Option<ReposAction>,
        #[arg(long)]
        json: bool,
    },
    /// Local web UI
    Serve {
        #[arg(long, default_value = "127.0.0.1:8790")]
        addr: String,
    },

    // —— power tools (hidden from default help; still callable) ——
    #[command(hide = true)]
    Timeline {
        path: PathBuf,
        #[arg(long, default_value_t = 30)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Blame {
        path: PathBuf,
        #[arg(long, default_value_t = 40)]
        lines: usize,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Map {
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Pr {
        query: String,
        #[arg(long, default_value_t = 12)]
        later: usize,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Ghosts {
        #[arg(long, default_value_t = 180)]
        days: i64,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Dig {
        pattern: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Pairs {
        #[arg(long, default_value = "365 days ago")]
        since: String,
        #[arg(long, default_value_t = 12)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true, alias = "radar")]
    Hunt {
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long, default_value = "180 days ago")]
        since: String,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Why {
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long, default_value = "90 days ago")]
        since: String,
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value_t = 8)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Heatmap {
        #[arg(long, default_value = "180 days ago")]
        since: String,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Blast {
        path: PathBuf,
        #[arg(long, default_value_t = 15)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Hotspots {
        #[arg(long, default_value = "180 days ago")]
        since: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Stale {
        #[arg(long, default_value_t = 180)]
        days: i64,
        #[arg(long, default_value_t = 30)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Contributors {
        #[arg(long, default_value = "365 days ago")]
        since: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Churn {
        #[arg(long, default_value = "365 days ago")]
        since: String,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true)]
    Tree {
        #[arg(default_value = "")]
        path: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ReposAction {
    /// Delete one cached clone (owner/repo or id)
    Rm {
        spec: String,
        #[arg(long, short = 'y', help = "do not prompt")]
        yes: bool,
    },
    /// Delete all cached clones under ~/.timeforge/repos
    Clean {
        #[arg(long, short = 'y', help = "required — refuse without it")]
        yes: bool,
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
        Some(Commands::Open {
            spec,
            update,
            repair,
        }) => {
            let (owner, name) = remote::parse_github_spec(spec)?;
            let repo = if *repair {
                repair_cache(&owner, &name)?
            } else {
                open_github(&owner, &name, *update || cli.update, false)?
            };
            println!("opened {}/{}", owner, name);
            println!("path  {}", repo.path().display());
            println!("tip   timeforge --repo {}/{} ask \"…\"", owner, name);
            return Ok(());
        }
        Some(Commands::Repos { action, json }) => {
            match action {
                None => {
                    let list = list_cached()?;
                    if *json {
                        report::print_json(&list);
                    } else {
                        report::print_cached(&list);
                    }
                }
                Some(ReposAction::Rm { spec, yes }) => {
                    if !yes {
                        eprintln!("Delete cached `{spec}` from disk? Re-run with --yes to confirm.");
                        std::process::exit(1);
                    }
                    let r = remove_cached(spec)?;
                    if *json {
                        report::print_json(&r);
                    } else {
                        println!("removed {}", r.id);
                        println!("path    {}", r.path);
                    }
                }
                Some(ReposAction::Clean { yes }) => {
                    if !yes {
                        eprintln!("This deletes ALL of ~/.timeforge/repos. Re-run with --yes.");
                        std::process::exit(1);
                    }
                    let removed = clean_cached()?;
                    if *json {
                        report::print_json(&removed);
                    } else {
                        println!("cleaned {} cached repo(s)", removed.len());
                        for r in &removed {
                            println!("  - {}", r.id);
                        }
                    }
                }
            }
            return Ok(());
        }
        None if cli.query.is_empty() => {
            use clap::CommandFactory;
            Cli::command().print_help().ok();
            println!();
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
        None => {
            let q = cli.query.join(" ");
            let h = bug_hunt(&repo, &q, None, "180 days ago")?;
            report::print_hunt(&h);
        }
        Some(Commands::Ask {
            query,
            path,
            since,
            json,
        }) => {
            let q = query.join(" ");
            let p = path.as_ref().map(|p| p.to_string_lossy().to_string());
            let h = bug_hunt(&repo, &q, p.as_deref(), &since)?;
            if json {
                report::print_json(&h);
            } else {
                report::print_hunt(&h);
            }
        }
        Some(Commands::Open { .. }) | Some(Commands::Repos { .. }) => unreachable!(),
        Some(Commands::Update { json }) => {
            let u = update_repo(&repo)?;
            if json {
                report::print_json(&u);
            } else {
                println!("update  {}", u.message);
                println!("path    {} (same_path={})", u.path, u.same_path);
                println!("mode    {}", u.mode);
                println!("HEAD    {} → {}", u.before, u.after);
                if u.partial {
                    println!("note    still marked partial — try: timeforge repair");
                }
            }
        }
        Some(Commands::Repair { json }) => {
            let u = repair_current(&repo)?;
            if json {
                report::print_json(&u);
            } else {
                println!("repair  {}", u.message);
                println!("path    {} (same_path={})", u.path, u.same_path);
                println!("mode    {}", u.mode);
                println!("HEAD    {} → {}", u.before, u.after);
            }
        }
        Some(Commands::Timeline { path, limit, json }) => {
            let t = file_timeline(&repo, &path.to_string_lossy(), limit)?;
            if json {
                report::print_json(&t);
            } else {
                report::print_timeline(&t);
            }
        }
        Some(Commands::Blame { path, lines, json }) => {
            let b = file_blame(&repo, &path.to_string_lossy(), lines)?;
            if json {
                report::print_json(&b);
            } else {
                report::print_blame(&b);
            }
        }
        Some(Commands::Map { path, json }) => {
            let m = blame_map(&repo, &path.to_string_lossy())?;
            if json {
                report::print_json(&m);
            } else {
                report::print_blame_map(&m);
            }
        }
        Some(Commands::Pr { query, later, json }) => {
            let p = pr_travel(&repo, &query, later)?;
            if json {
                report::print_json(&p);
            } else {
                report::print_pr(&p);
            }
        }
        Some(Commands::Ghosts { days, limit, json }) => {
            let g = ghost_authors(&repo, days, limit)?;
            if json {
                report::print_json(&g);
            } else {
                report::print_ghosts(&g);
            }
        }
        Some(Commands::Dig {
            pattern,
            limit,
            json,
        }) => {
            let d = dig_pattern(&repo, &pattern, limit)?;
            if json {
                report::print_json(&d);
            } else {
                report::print_dig(&d);
            }
        }
        Some(Commands::Pairs {
            since,
            limit,
            json,
        }) => {
            let p = fix_break_pairs(&repo, &since, limit)?;
            if json {
                report::print_json(&p);
            } else {
                report::print_pairs(&p);
            }
        }
        Some(Commands::Hunt {
            query,
            path,
            since,
            json,
        }) => {
            let p = path.as_ref().map(|p| p.to_string_lossy().to_string());
            let q = query.unwrap_or_default();
            let h = bug_hunt(&repo, &q, p.as_deref(), &since)?;
            if json {
                report::print_json(&h);
            } else {
                report::print_hunt(&h);
            }
        }
        Some(Commands::Why {
            path,
            since,
            query,
            limit,
            json,
        }) => {
            let p = path.as_ref().map(|p| p.to_string_lossy().to_string());
            let w = why_broke(&repo, p.as_deref(), &since, query.as_deref(), limit)?;
            if json {
                report::print_json(&w);
            } else {
                report::print_why(&w);
            }
        }
        Some(Commands::Heatmap { since, path, json }) => {
            let p = path.as_ref().map(|p| p.to_string_lossy().to_string());
            let h = ownership_heatmap(&repo, &since, p.as_deref())?;
            if json {
                report::print_json(&h);
            } else {
                report::print_heatmap(&h);
            }
        }
        Some(Commands::Blast { path, limit, json }) => {
            let b = blast_radius(&repo, &path.to_string_lossy(), limit)?;
            if json {
                report::print_json(&b);
            } else {
                report::print_blast(&b);
            }
        }
        Some(Commands::Hotspots { since, limit, json }) => {
            let h = file_hotspots(&repo, &since, limit)?;
            if json {
                report::print_json(&h);
            } else {
                report::print_hotspots(&h);
            }
        }
        Some(Commands::Stale { days, limit, json }) => {
            let s = stale_files(&repo, days, limit)?;
            if json {
                report::print_json(&s);
            } else {
                report::print_stale(&s);
            }
        }
        Some(Commands::Contributors { since, limit, json }) => {
            let c = contributors(&repo, &since, limit)?;
            if json {
                report::print_json(&c);
            } else {
                report::print_contributors(&c);
            }
        }
        Some(Commands::Churn { since, json }) => {
            let c = commit_churn(&repo, &since)?;
            if json {
                report::print_json(&c);
            } else {
                report::print_churn(&c);
            }
        }
        Some(Commands::Tree { path, json }) => {
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
        Some(Commands::Serve { addr }) => {
            timeforge::web::serve(&addr, repo.path())?;
        }
    }
    Ok(())
}
