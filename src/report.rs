use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Table};

use crate::activity::{ChurnReport, ContributorsReport};
use crate::blast::BlastReport;
use crate::blame::BlameReport;
use crate::heatmap::Heatmap;
use crate::hotspots::HotspotsReport;
use crate::remote::CachedRepo;
use crate::stale::StaleReport;
use crate::timeline::Timeline;
use crate::why::WhyReport;

pub fn print_timeline(t: &Timeline) {
    banner();
    println!("{}  {}", "TIME MACHINE".bold().cyan(), t.path.yellow());
    println!(
        "  {} commits · first: {} · last: {}\n",
        t.total_commits,
        t.first_author.as_deref().unwrap_or("?"),
        t.last_author.as_deref().unwrap_or("?")
    );
    for (i, e) in t.events.iter().enumerate() {
        let pr = e
            .pr_hint
            .as_ref()
            .map(|p| format!(" {}", p.magenta()))
            .unwrap_or_default();
        println!(
            "  {:>2}. {} {} {}  +{} -{}{}",
            i + 1,
            e.commit.short.green(),
            e.commit.date.dimmed(),
            e.commit.author.cyan(),
            e.insertions,
            e.deletions,
            pr
        );
        println!("      {}", e.commit.subject);
    }
    println!();
}

pub fn print_blame(b: &BlameReport) {
    banner();
    println!("{}  {} ({} lines)\n", "BLAME+".bold().cyan(), b.path.yellow(), b.total_lines);
    println!("{}", "OWNERS".bold().underline());
    for a in b.authors.iter().take(8) {
        let bar = "█".repeat((a.percent / 5.0) as usize);
        println!(
            "  {:24} {:>5} lines  {:>5.1}%  {}",
            a.author.cyan(),
            a.lines,
            a.percent,
            bar.dimmed()
        );
    }
    println!("\n{}", "SAMPLE LINES".bold().underline());
    for l in b.lines.iter().take(15) {
        println!(
            "  {:>4} {} {:16} {}",
            l.line,
            l.hash.green(),
            l.author.cyan(),
            truncate(&l.content, 60)
        );
    }
    println!();
}

pub fn print_why(w: &WhyReport) {
    banner();
    println!("{}  query: {}", "WHY DID IT BREAK?".bold().red(), w.query.yellow());
    if let Some(p) = &w.path_filter {
        println!("  path filter: {p}");
    }
    println!();
    for (i, s) in w.suspects.iter().enumerate() {
        println!(
            "  {:>2}. score {}  {}  {} — {}",
            i + 1,
            format!("{:>3}", s.score).red().bold(),
            s.commit.short.green(),
            s.commit.date.dimmed(),
            s.commit.subject
        );
        for r in &s.reasons {
            println!("      · {}", r.dimmed());
        }
        if !s.files_touched.is_empty() {
            println!(
                "      files: {}",
                s.files_touched.join(", ").dimmed()
            );
        }
    }
    println!("\n  {}\n", w.hint.dimmed());
}

pub fn print_heatmap(h: &Heatmap) {
    banner();
    println!("{}  since {}\n", "OWNERSHIP HEATMAP".bold().cyan(), h.since.yellow());
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Author", "Commits", "Files", "Risk"]);
    for o in &h.owners {
        table.add_row(vec![
            o.author.clone(),
            o.commits.to_string(),
            o.files.to_string(),
            o.risk.clone(),
        ]);
    }
    println!("{table}");
    println!("\n  Bus factor (est.): {}", h.bus_factor);
    if let Some(w) = &h.warning {
        println!("  {} {}", "⚠".red(), w.red());
    }
    println!();
}

pub fn print_blast(b: &BlastReport) {
    banner();
    println!("{}  {}\n", "BLAST RADIUS".bold().magenta(), b.path.yellow());
    println!("  {}\n", b.summary);
    for h in &b.related {
        let bar = "▓".repeat((h.affinity * 12.0) as usize);
        println!(
            "  {:>3}×  {:40} {}",
            h.co_changes,
            truncate(&h.path, 40),
            bar.dimmed()
        );
    }
    println!();
}

pub fn print_hotspots(h: &HotspotsReport) {
    banner();
    println!("{}  since {}\n", "HOTSPOTS".bold().yellow(), h.since.cyan());
    for (i, s) in h.hotspots.iter().enumerate() {
        println!(
            "  {:>2}. score {:>3}  {:>3} commits  {:>2} authors  {}",
            i + 1,
            s.score.to_string().red(),
            s.commits,
            s.authors,
            s.path
        );
    }
    println!();
}

pub fn print_stale(s: &StaleReport) {
    banner();
    println!(
        "{}  older than {} days\n",
        "STALE FILES".bold().magenta(),
        s.older_than_days
    );
    for f in &s.files {
        println!(
            "  {:>4}d  {}  {}  {}",
            f.age_days.to_string().yellow(),
            f.last_date.dimmed(),
            f.last_author.cyan(),
            f.path
        );
    }
    println!();
}

pub fn print_contributors(c: &ContributorsReport) {
    banner();
    println!(
        "{}  {} commits since {}\n",
        "CONTRIBUTORS".bold().cyan(),
        c.total_commits,
        c.since.yellow()
    );
    for (i, a) in c.contributors.iter().enumerate() {
        let bar = "█".repeat((a.percent / 4.0) as usize);
        println!(
            "  {:>2}. {:24} {:>5}  {:>5.1}%  {}",
            i + 1,
            a.author.cyan(),
            a.commits,
            a.percent,
            bar.dimmed()
        );
    }
    println!();
}

pub fn print_churn(c: &ChurnReport) {
    banner();
    println!("{}  since {}\n", "COMMIT CHURN".bold().cyan(), c.since.yellow());
    let max = c.buckets.iter().map(|b| b.commits).max().unwrap_or(1).max(1);
    for b in &c.buckets {
        let w = ((b.commits as f64 / max as f64) * 24.0) as usize;
        println!(
            "  {}  {:>4}  {}",
            b.week,
            b.commits,
            "▓".repeat(w).dimmed()
        );
    }
    println!();
}

pub fn print_cached(list: &[CachedRepo]) {
    banner();
    println!("{}\n", "CACHED REMOTE REPOS".bold().cyan());
    if list.is_empty() {
        println!("  (empty) — try: timeforge open rust-lang/mdBook\n");
        return;
    }
    for r in list {
        println!("  {}  {}", r.id.green(), r.path.dimmed());
        if let Some(remote) = &r.remote {
            println!("      {}", remote.dimmed());
        }
    }
    println!();
}

pub fn print_json<T: serde::Serialize>(v: &T) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn banner() {
    println!(
        "{}",
        r#"
  _____ _                      __                   
 |_   _(_)_ __  ___  / _| ___  _ __ __ _  ___ 
   | | | | '_ \/ _ \| |_ / _ \| '__/ _` |/ _ \
   | | | | | | |  __/|  _| (_) | | | (_| |  __/
   |_| |_|_| |_|\___||_|  \___/|_|  \__, |\___|
      repo time machine · FounderB   |___/      
"#
        .cyan()
    );
}

fn truncate(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…")
    } else {
        t
    }
}
