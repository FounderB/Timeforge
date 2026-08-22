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
timeforge open owner/repo          # clone (blobless) + cache
timeforge open owner/repo --update # fetch latest
timeforge repos                    # list ~/.timeforge/repos
timeforge --repo owner/repo why --query fix
```

---

## CLI (v0.2)

```bash
timeforge open <spec>
timeforge repos
timeforge timeline <path>
timeforge blame <path>
timeforge why [--path …] [--query …]
timeforge heatmap
timeforge blast <path>
timeforge hotspots          # highest churn files
timeforge stale --days 180  # untouched files
timeforge contributors
timeforge churn             # weekly commit chart
timeforge serve
```

Global: `--repo <path|owner/repo|url>` · `--update`

---

## Features

1. **Time Machine** — file history + PR hints  
2. **Why broke** — ranked culprit commits  
3. **Heatmap** — ownership / bus factor  
4. **Blast radius** — co-change affinity  
5. **Hotspots** — files with most churn  
6. **Stale** — abandoned paths  
7. **Contributors** / **Churn** — people & tempo  
8. **Remote open** — analyze any public GitHub repo  

---

## License

MIT
