use std::process::Command;
use std::sync::Mutex;
use tempfile::tempdir;

static HOME_LOCK: Mutex<()> = Mutex::new(());

fn git(cwd: &std::path::Path, args: &[&str]) {
    assert!(Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Dev")
        .env("GIT_AUTHOR_EMAIL", "dev@example.com")
        .env("GIT_COMMITTER_NAME", "Dev")
        .env("GIT_COMMITTER_EMAIL", "dev@example.com")
        .status()
        .unwrap()
        .success());
}

#[test]
fn remove_cached_only_under_timeforge() {
    let _g = HOME_LOCK.lock().unwrap();
    let home = tempdir().unwrap();
    std::env::set_var("HOME", home.path());

    let cache = home.path().join(".timeforge/repos/Acme_Demo");
    std::fs::create_dir_all(&cache).unwrap();
    git(&cache, &["init"]);
    git(&cache, &["config", "user.email", "dev@example.com"]);
    git(&cache, &["config", "user.name", "Dev"]);
    std::fs::write(cache.join("README.md"), "x\n").unwrap();
    git(&cache, &["add", "."]);
    git(&cache, &["commit", "-m", "init"]);

    assert!(
        cache.join(".git").exists(),
        "git dir missing at {}",
        cache.display()
    );
    let list = timeforge::list_cached().unwrap();
    assert!(
        list.iter().any(|r| r.id == "Acme_Demo"),
        "list={list:?} home={:?}",
        home.path()
    );

    let r = timeforge::remove_cached("Acme/Demo").unwrap();
    assert!(r.ok);
    assert!(!cache.exists());
    assert!(timeforge::list_cached().unwrap().is_empty());
}

#[test]
fn remove_refuses_outside_cache() {
    let _g = HOME_LOCK.lock().unwrap();
    let home = tempdir().unwrap();
    std::env::set_var("HOME", home.path());
    let outsider = home.path().join("not-cache");
    std::fs::create_dir_all(&outsider).unwrap();
    git(&outsider, &["init"]);
    let err = timeforge::remove_cached("../not-cache").unwrap_err();
    assert!(
        err.contains("could not resolve") || err.contains("refusing") || err.contains("not cached"),
        "{err}"
    );
}
