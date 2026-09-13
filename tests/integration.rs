use std::process::Command;
use tempfile::tempdir;

fn git(cwd: &std::path::Path, args: &[&str]) {
    let st = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Dev")
        .env("GIT_AUTHOR_EMAIL", "dev@example.com")
        .env("GIT_COMMITTER_NAME", "Dev")
        .env("GIT_COMMITTER_EMAIL", "dev@example.com")
        .env_remove("GIT_AUTHOR_DATE")
        .env_remove("GIT_COMMITTER_DATE")
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
fn why_downranks_fix_as_culprit() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let w = timeforge::why_broke(&repo, Some("src"), "10 years ago", Some("auth"), 5).unwrap();
    assert!(!w.suspects.is_empty());
    let fix = w
        .suspects
        .iter()
        .find(|s| s.commit.subject.to_lowercase().contains("fix"));
    let add = w
        .suspects
        .iter()
        .find(|s| s.commit.subject.to_lowercase().contains("add auth"));
    if let (Some(fix), Some(add)) = (fix, add) {
        assert!(
            add.score >= fix.score,
            "introduce should outrank repair: add={} fix={}",
            add.score,
            fix.score
        );
        assert!(
            fix.reasons.iter().any(|r| r.contains("down-ranked")),
            "fix should be marked down-ranked"
        );
    }
}

#[test]
fn fix_break_uses_pickaxe() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    git(root, &["init"]);
    git(root, &["config", "user.email", "dev@example.com"]);
    git(root, &["config", "user.name", "Dev"]);
    write(root, "src/pay.rs", "fn charge() { ok() }\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "scaffold"]);
    write(
        root,
        "src/pay.rs",
        "fn charge() { UNIQUE_BUG_MARKER_XYZ(); }\n",
    );
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "add payment path"]);
    write(root, "src/pay.rs", "fn charge() { ok() }\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "fix payment regression"]);

    let repo = timeforge::Repo::discover(root).unwrap();
    let p = timeforge::fix_break_pairs(&repo, "10 years ago", 5).unwrap();
    assert!(!p.pairs.is_empty());
    let top = &p.pairs[0];
    assert!(top.fix.subject.to_lowercase().contains("fix"));
    assert!(
        top.method == "pickaxe" || top.method == "blame",
        "expected hunk causality, got {}",
        top.method
    );
    let br = top.break_commit.as_ref().expect("break commit");
    assert!(
        br.subject.to_lowercase().contains("add payment")
            || br.subject.to_lowercase().contains("payment"),
        "break subject was {}",
        br.subject
    );
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

#[test]
fn tree_lists_dirs_and_files() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let root = timeforge::list_tree(&repo, "").unwrap();
    assert!(root
        .entries
        .iter()
        .any(|e| e.name == "src" && e.kind == "tree"));
    assert!(root
        .entries
        .iter()
        .any(|e| e.name == "README.md" && e.kind == "blob"));
    let src = timeforge::list_tree(&repo, "src").unwrap();
    assert!(src.entries.iter().any(|e| e.name == "auth.rs"));
    assert_eq!(src.parent.as_deref(), Some(""));
}

#[test]
fn blame_map_has_zones() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let m = timeforge::blame_map(&repo, "src/auth.rs").unwrap();
    assert!(!m.blocks.is_empty());
    assert!(m.total_lines >= 1);
}

#[test]
fn pr_travel_finds_hash_mention() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let p = timeforge::pr_travel(&repo, "12", 5).unwrap();
    assert!(p.pr.as_deref() == Some("#12"));
    assert!(!p.files.is_empty());
}

#[test]
fn dig_finds_real_pattern_and_rejects_noise() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    // "login" was introduced in auth.rs
    let hit = timeforge::dig_pattern(&repo, "login", 10).unwrap();
    assert!(!hit.events.is_empty(), "expected dig hits for login");
    let miss = timeforge::dig_pattern(&repo, "zzznofindpattern999", 10).unwrap();
    assert!(miss.events.is_empty());
}

#[test]
fn hunt_ranks_auth_and_ignores_garbage() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let good = timeforge::bug_hunt(&repo, "auth", Some("src"), "10 years ago").unwrap();
    assert!(!good.hits.is_empty());
    let a = good.answer.expect("answer");
    assert!(!a.evidence.is_empty());
    assert!(!a.method.is_empty());
    assert!(good.elapsed_ms < 30_000);
    assert!(
        good.hits.iter().any(|h| h.score >= 25),
        "expected strong hits for auth: {:?}",
        good.hits
    );
    let bad = timeforge::bug_hunt(&repo, "qqqqqqqqnofind", None, "10 years ago").unwrap();
    assert!(
        bad.hits
            .iter()
            .all(|h| h.kind != "suspect" || h.score >= 18),
        "garbage query should not produce weak suspects: {:?}",
        bad.hits
    );
    assert!(bad
        .dig
        .as_ref()
        .map(|d| d.events.is_empty())
        .unwrap_or(true));
}

#[test]
fn hunt_pr_fast_path() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let h = timeforge::bug_hunt(&repo, "#12", None, "10 years ago").unwrap();
    assert_eq!(h.mode, "pr-fast");
    let a = h.answer.expect("answer");
    assert_eq!(a.method, "pr");
    assert_eq!(a.evidence, "strong");
    assert!(h.pr.is_some());
    assert!(h.elapsed_ms < 10_000);
}

#[test]
fn hunt_answer_prefers_dig_for_code_token() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let h = timeforge::bug_hunt(&repo, "login", None, "10 years ago").unwrap();
    let a = h.answer.expect("answer");
    assert_eq!(a.method, "dig");
    assert!(a.evidence == "proven" || a.evidence == "strong");
    assert!(a
        .drilldowns
        .iter()
        .any(|d| d.kind == "dig" || d.kind == "timeline"));
}

#[test]
fn hunt_answer_prefers_pickaxe_on_bug_ask() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    git(root, &["init"]);
    git(root, &["config", "user.email", "dev@example.com"]);
    git(root, &["config", "user.name", "Dev"]);
    write(root, "src/pay.rs", "fn charge() { ok() }\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "scaffold"]);
    write(
        root,
        "src/pay.rs",
        "fn charge() { UNIQUE_BUG_MARKER_XYZ(); }\n",
    );
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "add payment path"]);
    write(root, "src/pay.rs", "fn charge() { ok() }\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "fix payment regression"]);

    let repo = timeforge::Repo::discover(root).unwrap();
    let h = timeforge::bug_hunt(&repo, "payment panic", None, "10 years ago").unwrap();
    let a = h.answer.expect("answer");
    assert!(
        a.method == "pickaxe" || a.method == "blame" || a.method == "dig",
        "got method={} evidence={}",
        a.method,
        a.evidence
    );
    if a.method == "pickaxe" {
        assert_eq!(a.evidence, "proven");
    }
}

#[test]
fn pairs_finds_fix_commit() {
    let dir = seed_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let p = timeforge::fix_break_pairs(&repo, "10 years ago", 10).unwrap();
    assert!(!p.pairs.is_empty(), "seed has 'fix auth panic' commit");
    assert!(p.pairs.iter().any(|x| x.fix.subject.contains("fix")));
}
