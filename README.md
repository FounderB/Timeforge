# Timeforge

**Repo Time Machine for GitHub — local or any remote repo.**

See what changed, who owns it, why it broke — on *your* machine or any `owner/repo`.

Part of **FounderB**: [FluxTap](https://github.com/FounderB/FluxTap) · [Tracefuse](https://github.com/FounderB/Tracefuse) · [SignShield](https://github.com/FounderB/SignShield) · **Timeforge**

---

## Quick start

```bash
git clone https://github.com/FounderB/Timeforge.git
cd Timeforge
cargo build --release

# local repo (cwd)
./target/release/timeforge timeline README.md
./target/release/timeforge hotspots
./target/release/timeforge heatmap

# ANY GitHub repo (cloned to ~/.timeforge/repos)
./target/release/timeforge open FounderB/SignShield
./target/release/timeforge --repo FounderB/SignShield timeline README.md
./target/release/timeforge --repo rust-lang/mdBook contributors

# web UI — open remotes from the browser
./target/release/timeforge serve
# → http://127.0.0.1:8790
```

---

## Remote repos

| Spec | Example |
|------|---------|
| `owner/repo` | `FounderB/SignShield` |
| HTTPS | `https://github.com/rust-lang/mdBook` |
| Local path | `/home/you/code/app` |

```bash
timeforge open owner/repo          # full clone + cache (~/.timeforge/repos)
timeforge open owner/repo --update # fetch latest
timeforge open owner/repo --repair # fix broken partial/promisor cache (alias: --full)
timeforge repos                    # list ~/.timeforge/repos
timeforge --repo owner/repo why --query fix
```

If you see `promisor` / `Couldn't connect to github.com` errors, the cache was a partial clone. Run:

```bash
timeforge open owner/repo --repair
```

---

## CLI (v0.5)

```bash
timeforge ask / hunt --query unwrap   # one-question fast radar (~10ms local)
timeforge hunt --query '#12'          # PR fast path
timeforge update | repair             # in-place only
timeforge serve                       # Ask box + online pill
```

Global: `--repo <path|owner/repo|url>` · `--update`

---

## Features

1. **Bug Hunt / Radar** — one-shot suspects + dig + pairs + ghosts  
2. **Archaeology** — when a code pattern was born  
3. **Fix↔Break** — link fixes to earlier culprits  
4. **Update** — sync repo to latest remote  
5. **Time Machine** — file history + rename follow  
6. **Blame Map** — colored ownership zones  
7. **PR Time Travel** — PR files + later churn  
8. **Ghost authors** — silent owners + path bus-factor  
9. **Why / Heatmap / Blast / Hotspots / Stale / Churn**  
10. **Remote open** + **explorer UI** (pins, sparks, keyboard)  


## License

MIT
