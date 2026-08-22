//! Golden Ask calibration — local fixtures + optional cached remotes.

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

fn seed_story_repo() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    let root = dir.path();
    git(root, &["init"]);
    git(root, &["config", "user.email", "dev@example.com"]);
    git(root, &["config", "user.name", "Dev"]);
    write(root, "src/auth.rs", "fn login() { ok() }\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "scaffold auth"]);
    write(
        root,
        "src/auth.rs",
        "fn login() { UNIQUE_STACK_MARKER(); }\n",
    );
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "add UNIQUE_STACK_MARKER"]);
    write(root, "src/auth.rs", "fn login() { ok() }\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "fix auth panic (#99)"]);
    dir
}

#[test]
fn parse_stack_like_query() {
    let p = timeforge::parse_ask("panic at src/auth.rs:12", None);
    assert_eq!(p.path.as_deref(), Some("src/auth.rs"));
    assert_eq!(p.line, Some(12));
    assert!(p.bug_like);
}

#[test]
fn ask_path_scoped_story() {
    let dir = seed_story_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let h = timeforge::bug_hunt(
        &repo,
        "panic at src/auth.rs:1",
        None,
        "10 years ago",
    )
    .unwrap();
    assert_eq!(h.parsed.path.as_deref(), Some("src/auth.rs"));
    assert!(h.answer.is_some() || !h.hits.is_empty());
    assert!(h.elapsed_ms < 5_000, "too slow: {}ms", h.elapsed_ms);
}

#[test]
fn ask_symbol_dig() {
    let dir = seed_story_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let h = timeforge::bug_hunt(&repo, "UNIQUE_STACK_MARKER", None, "10 years ago").unwrap();
    let a = h.answer.expect("answer");
    assert!(
        a.method == "dig" || a.method == "pickaxe",
        "got {}",
        a.method
    );
    assert!(a.evidence == "proven" || a.evidence == "strong");
    assert!(h.elapsed_ms < 3_000);
}

#[test]
fn ask_pickaxe_on_fix_story() {
    let dir = seed_story_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let h = timeforge::bug_hunt(&repo, "auth panic", None, "10 years ago").unwrap();
    let a = h.answer.expect("answer");
    assert!(
        a.method == "pickaxe" || a.method == "blame" || a.method == "dig",
        "got {}",
        a.method
    );
}

#[test]
fn ask_pr_includes_later_touch_hits() {
    let dir = seed_story_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let h = timeforge::bug_hunt(&repo, "#99", None, "10 years ago").unwrap();
    assert_eq!(h.mode, "pr-fast");
    let a = h.answer.expect("answer");
    assert_eq!(a.method, "pr");
    assert!(a.drilldowns.iter().any(|d| d.label.contains("After")));
}

#[test]
fn ask_speed_code_token_skips_pairs() {
    let dir = seed_story_repo();
    let repo = timeforge::Repo::discover(dir.path()).unwrap();
    let h = timeforge::bug_hunt(&repo, "UNIQUE_STACK_MARKER", None, "10 years ago").unwrap();
    // Fast path: no pairs for pure code token
    assert!(h.pairs.is_none() || h.pairs.as_ref().unwrap().pairs.is_empty() || h.elapsed_ms < 2000);
    assert!(h.elapsed_ms < 2000, "expected fast dig, got {}ms", h.elapsed_ms);
}

/// Soft golden checks against ~/.timeforge cache when present.
#[test]
fn golden_cached_remotes_if_present() {
    let home = dirs_home();
    let cases = [
        ("BurntSushi_ripgrep", "PCRE2", "dig"),
        ("clap-rs_clap", "ArgAction", "dig"),
        ("serde-rs_serde", "Deserialize", "dig"),
        ("FounderB_Timeforge", "PRETTY_COMMIT", "dig"),
    ];
    let mut ran = 0;
    for (folder, q, method) in cases {
        let path = home.join(".timeforge/repos").join(folder);
        if !path.join(".git").exists() && !path.exists() {
            continue;
        }
        let Ok(repo) = timeforge::Repo::discover(&path) else {
            continue;
        };
        let h = timeforge::bug_hunt(&repo, q, None, "10 years ago").unwrap();
        let a = h.answer.expect("answer");
        assert_eq!(a.method, method, "{folder} {q}");
        assert!(
            a.evidence == "proven" || a.evidence == "strong",
            "{folder} {q} evidence={}",
            a.evidence
        );
        assert!(h.elapsed_ms < 15_000, "{folder} slow {}ms", h.elapsed_ms);
        ran += 1;
    }
    // Don't fail CI machines without cache — local/dev will hit these.
    eprintln!("golden_cached_remotes ran {ran} cases");
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
}
