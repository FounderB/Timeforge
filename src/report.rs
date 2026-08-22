use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Table};

use crate::blast::BlastReport;
use crate::blame::BlameReport;
use crate::heatmap::Heatmap;
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
