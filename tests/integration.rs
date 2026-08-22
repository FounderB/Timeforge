use std::process::Command;
use tempfile::tempdir;

fn git(cwd: &std::path::Path, args: &[&str]) {
    let st = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(st.success(), "git {:?} failed", args);
}

fn write(cwd: &std::path::Path, rel: &str, body: &str) {
    let p = cwd.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&p, body).unwrap();
}

fn seed_repo() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    let root = dir.path();
    git(root, &["init"]);
    git(root, &["config", "user.email", "dev@example.com"]);
    git(root, &["config", "user.name", "Dev"]);
    write(root, "README.md", "# demo\n");
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-m", "initial"]);
    write(root, "src/auth.rs", "fn login() {}\n");
    git(root, &["add", "src/auth.rs"]);
    git(root, &["commit", "-m", "add auth"]);
    write(root, "src/auth.rs", "fn login() { /* hotfix */ }\n");
    write(root, "src/main.rs", "fn main() {}\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "fix auth panic (#12)"]);
    dir
}

#[test]
fn timeline_has_events() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let t = timeforge::file_timeline(&repo, "src/auth.rs", 20).unwrap();
    assert!(t.total_commits >= 2);
    assert!(t.events.iter().any(|e| e.pr_hint.as_deref() == Some("#12")));
}

#[test]
fn why_ranks_hotfix() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let w = timeforge::why_broke(&repo, Some("src"), "10 years ago", Some("auth"), 5).unwrap();
    assert!(!w.suspects.is_empty());
    assert!(w.suspects[0].score >= w.suspects.last().unwrap().score);
}

#[test]
fn heatmap_lists_owner() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let h = timeforge::ownership_heatmap(&repo, "10 years ago", None).unwrap();
    assert!(!h.owners.is_empty());
    assert_eq!(h.owners[0].author, "Dev");
}

#[test]
fn blast_finds_cochange() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let b = timeforge::blast_radius(&repo, "src/auth.rs", 10).unwrap();
    assert!(b.related.iter().any(|h| h.path.contains("main.rs")));
}

#[test]
fn parse_github_specs() {
    let (o, n) = timeforge::remote::parse_github_spec("FounderB/SignShield").unwrap();
    assert_eq!(o, "FounderB");
    assert_eq!(n, "SignShield");
    let (o, n) =
        timeforge::remote::parse_github_spec("https://github.com/rust-lang/mdBook.git").unwrap();
    assert_eq!(o, "rust-lang");
    assert_eq!(n, "mdBook");
}

#[test]
fn hotspots_and_contributors() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let h = timeforge::file_hotspots(&repo, "10 years ago", 10).unwrap();
    assert!(!h.hotspots.is_empty());
    let c = timeforge::contributors(&repo, "10 years ago", 5).unwrap();
    assert_eq!(c.contributors[0].author, "Dev");
}
