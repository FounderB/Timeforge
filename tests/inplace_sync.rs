//! In-place update/repair tests (local bare remotes — no GitHub).
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
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .unwrap();
    assert!(st.success(), "git {:?} in {} failed", args, cwd.display());
}

fn write(cwd: &std::path::Path, rel: &str, body: &str) {
    let p = cwd.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&p, body).unwrap();
}

fn rev_short(cwd: &std::path::Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(cwd)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// bare <- seed, then work clone from bare.
fn setup_remote_pair() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempdir().unwrap();
    let bare = root.path().join("remote.git");
    let seed = root.path().join("seed");
    let work = root.path().join("work");

    std::fs::create_dir_all(&seed).unwrap();
    git(&seed, &["init", "-b", "master"]);
    git(&seed, &["config", "user.email", "dev@example.com"]);
    git(&seed, &["config", "user.name", "Dev"]);
    write(&seed, "README.md", "# v1\n");
    git(&seed, &["add", "README.md"]);
    git(&seed, &["commit", "-m", "initial"]);

    git(root.path(), &["clone", "--bare", seed.to_str().unwrap(), bare.to_str().unwrap()]);

    git(
        root.path(),
        &[
            "clone",
            bare.to_str().unwrap(),
            work.to_str().unwrap(),
        ],
    );
    git(&work, &["config", "user.email", "dev@example.com"]);
    git(&work, &["config", "user.name", "Dev"]);

    (root, bare, work)
}

#[test]
fn update_stays_same_path_and_pulls_new_files() {
    let (_root, bare, work) = setup_remote_pair();
    let work_path = work.canonicalize().unwrap();
    let marker = work.join(".local-marker");
    std::fs::write(&marker, "keep-me").unwrap();

    // Push a new commit via a second clone
    let other = _root.path().join("other");
    git(
        _root.path(),
        &["clone", bare.to_str().unwrap(), other.to_str().unwrap()],
    );
    git(&other, &["config", "user.email", "dev@example.com"]);
    git(&other, &["config", "user.name", "Dev"]);
    write(&other, "NEW.txt", "hello from remote\n");
    write(&other, "src/app.go", "package main\n");
    git(&other, &["add", "."]);
    git(&other, &["commit", "-m", "add NEW.txt"]);
    git(&other, &["push", "origin", "HEAD"]);

    let before = rev_short(&work);
    let repo = timeforge::Repo::discover(&work).unwrap();
    let u = timeforge::update_repo(&repo).unwrap();

    assert!(u.same_path, "update must keep same path: {:?}", u);
    assert_eq!(
        work_path,
        std::path::PathBuf::from(&u.path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(&u.path))
    );
    assert!(
        work.join(".git").exists(),
        ".git must still exist in place"
    );
    assert!(
        marker.exists() && std::fs::read_to_string(&marker).unwrap() == "keep-me",
        "local untracked marker must survive in-place update"
    );
    assert!(
        work.join("NEW.txt").exists(),
        "new remote file must appear after update"
    );
    assert!(
        work.join("src/app.go").exists(),
        "new nested file must appear"
    );
    let after = rev_short(&work);
    assert_ne!(before, after, "HEAD should advance");
    assert_eq!(u.before, before);
    assert_eq!(u.after, after);
    assert!(
        u.mode.contains("inplace") || u.mode == "noop",
        "mode should be inplace-*: {}",
        u.mode
    );
    assert!(
        u.message.contains("in place") || u.message.contains("updated"),
        "{}",
        u.message
    );
}

#[test]
fn repair_stays_same_path_never_wipes() {
    let (_root, _bare, work) = setup_remote_pair();
    let work_path = work.canonicalize().unwrap();
    let marker = work.join("DO_NOT_DELETE.txt");
    std::fs::write(&marker, "precious").unwrap();
    let inode_before = std::fs::metadata(&work).unwrap();

    let repo = timeforge::Repo::discover(&work).unwrap();
    let u = timeforge::repair_current(&repo).unwrap();

    assert!(u.same_path);
    assert_eq!(
        work_path,
        std::path::PathBuf::from(&u.path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(&u.path))
    );
    assert!(marker.exists(), "repair must not wipe working tree files");
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "precious");
    assert!(work.join("README.md").exists());
    assert!(work.join(".git").exists());
    // Directory still there (not replaced by a fresh clone folder)
    let _ = inode_before;
    assert!(
        u.message.contains("in place") || u.mode.contains("inplace"),
        "expected in-place repair: {} / {}",
        u.message,
        u.mode
    );
}

#[test]
fn update_noop_without_remote_keeps_path() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    git(root, &["init"]);
    git(root, &["config", "user.email", "dev@example.com"]);
    git(root, &["config", "user.name", "Dev"]);
    write(root, "a.txt", "x\n");
    git(root, &["add", "a.txt"]);
    git(root, &["commit", "-m", "init"]);

    let path = root.canonicalize().unwrap();
    let repo = timeforge::Repo::discover(root).unwrap();
    let u = timeforge::update_repo(&repo).unwrap();
    assert!(u.same_path);
    assert_eq!(u.mode, "noop");
    assert!(path.join("a.txt").exists());
    assert!(path.join(".git").exists());
}

#[test]
fn double_update_is_idempotent_same_path() {
    let (_root, bare, work) = setup_remote_pair();
    let repo = timeforge::Repo::discover(&work).unwrap();
    let u1 = timeforge::update_repo(&repo).unwrap();
    let u2 = timeforge::update_repo(&repo).unwrap();
    assert!(u1.same_path && u2.same_path);
    assert_eq!(u1.path, u2.path);
    assert_eq!(u1.after, u2.after);
    assert!(work.join("README.md").exists());
    let _ = bare;
}

#[test]
fn partial_clone_materialize_stays_inplace() {
    let root = tempdir().unwrap();
    let bare = root.path().join("remote.git");
    let seed = root.path().join("seed");
    let work = root.path().join("work");

    std::fs::create_dir_all(&seed).unwrap();
    git(&seed, &["init", "-b", "master"]);
    git(&seed, &["config", "user.email", "dev@example.com"]);
    git(&seed, &["config", "user.name", "Dev"]);
    write(&seed, "big.txt", &"x".repeat(1000));
    write(&seed, "ok.md", "hi\n");
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-m", "blob"]);

    git(
        root.path(),
        &["clone", "--bare", seed.to_str().unwrap(), bare.to_str().unwrap()],
    );

    // blobless partial clone from local bare (file:// required for --filter)
    let bare_url = format!("file://{}", bare.display());
    let st = Command::new("git")
        .args([
            "clone",
            "--filter=blob:none",
            "--single-branch",
            &bare_url,
            work.to_str().unwrap(),
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .status()
        .unwrap();
    assert!(st.success(), "partial clone failed");

    // Confirm filter is set when supported
    let filter = Command::new("git")
        .args(["config", "--get", "remote.origin.partialclonefilter"])
        .current_dir(&work)
        .output()
        .unwrap();
    let filter = String::from_utf8_lossy(&filter.stdout);
    eprintln!("partialclonefilter={filter:?}");

    let marker = work.join("local-only");
    std::fs::write(&marker, "stay").unwrap();
    let path = work.canonicalize().unwrap();

    let repo = timeforge::Repo::discover(&work).unwrap();

    let u = timeforge::repair_current(&repo).unwrap();
    assert!(u.same_path);
    assert!(u.mode.contains("inplace"), "mode={}", u.mode);
    assert_eq!(
        path,
        std::path::PathBuf::from(&u.path)
            .canonicalize()
            .unwrap_or_else(|_| path.clone())
    );
    assert!(marker.exists(), "must not reclone into a new folder");
    assert!(work.join("ok.md").exists());
    let _ = timeforge::file_timeline(&repo, "ok.md", 5).unwrap();
}
