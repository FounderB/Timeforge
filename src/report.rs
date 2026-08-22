use colored::Colorize;
use comfy_table::{presets::UTF8_FULL, Table};

use crate::activity::{ChurnReport, ContributorsReport};
use crate::archaeology::ArchaeologyReport;
use crate::blame::BlameReport;
use crate::blame_map::BlameMap;
use crate::blast::BlastReport;
use crate::fixbreak::FixBreakReport;
use crate::ghosts::GhostReport;
use crate::heatmap::Heatmap;
use crate::hotspots::HotspotsReport;
use crate::hunt::HuntReport;
use crate::pr::PrTravel;
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

pub fn print_blame_map(m: &BlameMap) {
    banner();
    println!("{}  {}\n", "BLAME MAP".bold().cyan(), m.path.yellow());
    println!("  {}\n", m.summary.dimmed());
    let total = m.total_lines.max(1) as f64;
    for b in &m.blocks {
        let w = ((b.lines as f64 / total) * 28.0).round().max(1.0) as usize;
        println!(
            "  L{:>4}-L{:<4} {:16} {}  {}",
            b.start,
            b.end,
            b.author.cyan(),
            "█".repeat(w).green(),
            b.hash.dimmed()
        );
        println!("           {}", truncate(&b.preview, 70).dimmed());
    }
    println!();
}

pub fn print_pr(p: &PrTravel) {
    banner();
    println!(
        "{}  {}\n",
        "PR TIME TRAVEL".bold().magenta(),
        p.pr.as_deref().unwrap_or(&p.query).yellow()
    );
    println!("  {}\n", p.summary);
    println!(
        "  {} {} {} — {}",
        p.commit.short.green(),
        p.commit.date.dimmed(),
        p.commit.author.cyan(),
        p.commit.subject
    );
    println!("\n{}", "FILES".bold().underline());
    for f in p.files.iter().take(20) {
        println!(
            "  +{:<4} -{:<4}  {}",
            f.insertions, f.deletions, f.path
        );
    }
    if !p.later.is_empty() {
        println!("\n{}", "LATER TOUCHES".bold().underline());
        for l in &p.later {
            println!(
                "  {} {}  {}  {}",
                l.commit.short.green(),
                l.commit.date.dimmed(),
                l.path.yellow(),
                truncate(&l.commit.subject, 50)
            );
        }
    }
    println!();
}

pub fn print_ghosts(g: &GhostReport) {
    banner();
    println!("{}\n  {}\n", "GHOST AUTHORS".bold().red(), g.summary.dimmed());
    for a in &g.ghosts {
        println!(
            "  [{:^6}] {:20} silent {}d · last {} · {} files",
            a.risk.yellow(),
            a.author.cyan(),
            a.days_silent,
            a.last_commit.dimmed(),
            a.owned_files
        );
        if !a.sample_paths.is_empty() {
            println!("           {}", a.sample_paths.join(", ").dimmed());
        }
    }
    if !g.path_risks.is_empty() {
        println!("\n{}", "PATH BUS FACTOR".bold().underline());
        for p in g.path_risks.iter().take(15) {
            println!(
                "  bus~{}  {:>5.0}% {:16}  {}",
                p.bus_factor,
                p.top_percent,
                p.top_author.cyan(),
                p.path
            );
        }
    }
    println!();
}

pub fn print_dig(d: &ArchaeologyReport) {
    banner();
    println!("{}  `{}`\n", "BUG ARCHAEOLOGY".bold().yellow(), d.pattern.cyan());
    println!("  {}\n", d.summary.dimmed());
    for e in &d.events {
        println!(
            "  [{}] {} {} {} — {}",
            e.kind.magenta(),
            e.commit.short.green(),
            e.commit.date.dimmed(),
            e.commit.author.cyan(),
            e.commit.subject
        );
        if !e.files.is_empty() {
            println!("        {}", e.files.join(", ").dimmed());
        }
    }
    println!();
}

pub fn print_pairs(p: &FixBreakReport) {
    banner();
    println!("{}\n  {}\n", "FIX ↔ BREAK".bold().red(), p.summary.dimmed());
    for (i, pair) in p.pairs.iter().enumerate() {
        println!(
            "  {:>2}. conf {}  [{}] fix {} — {}",
            i + 1,
            format!("{:>2}", pair.confidence).yellow(),
            pair.method.cyan(),
            pair.fix.short.green(),
            pair.fix.subject
        );
        if let Some(b) = &pair.break_commit {
            println!(
                "      break {} {} — {}",
                b.short.red(),
                b.date.dimmed(),
                b.subject
            );
        }
        println!("      {}", pair.note.dimmed());
        if !pair.shared_files.is_empty() {
            println!("      files: {}", pair.shared_files.join(", ").dimmed());
        }
    }
    println!();
}

pub fn print_hunt(h: &HuntReport) {
    banner();
    println!(
        "{}\n  {} · {}ms · mode {}\n",
        "ASK".bold().red(),
        h.summary.dimmed(),
        h.elapsed_ms,
        h.mode.cyan()
    );
    if let Some(a) = &h.answer {
        println!("{}", "ANSWER".bold().underline());
        println!("  {}", a.headline.green().bold());
        println!(
            "  [{}] {} · confidence {}",
            a.evidence.yellow().bold(),
            a.method.cyan(),
            a.confidence
        );
        println!("  {}", a.why.dimmed());
        if let Some(c) = &a.commit {
            println!("  commit {}", c.yellow());
        }
        if let Some(p) = &a.path {
            println!("  path   {}", p.yellow());
        }
        if !a.drilldowns.is_empty() {
            println!("  {}", "drill-down".bold());
            for d in &a.drilldowns {
                println!("    · {} ({})", d.label, d.kind.dimmed());
            }
        }
        println!();
    }
    for hit in &h.hits {
        println!(
            "  {:>3}  [{:^7}] [{:^10}] {}",
            hit.score.to_string().yellow(),
            hit.evidence.dimmed(),
            hit.kind.cyan(),
            hit.title
        );
        if !hit.detail.is_empty() {
            println!("        {}", hit.detail.dimmed());
        }
        if let Some(p) = &hit.path {
            println!("        → {}", p.yellow());
        }
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
    println!(
        "\n  tip  timeforge repos rm owner/repo --yes · timeforge repos clean --yes\n"
    );
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
