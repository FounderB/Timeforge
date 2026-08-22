# Timeforge

**Repo Time Machine for GitHub — see what changed, who owns it, and why it broke.**

Part of the **FounderB** stack: [FluxTap](https://github.com/FounderB/FluxTap) · [Tracefuse](https://github.com/FounderB/Tracefuse) · [SignShield](https://github.com/FounderB/SignShield) · **Timeforge**

<p align="center">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-1.70+-5eead4?style=for-the-badge&logo=rust"/>
  <img alt="License" src="https://img.shields.io/badge/License-MIT-38bdf8?style=for-the-badge"/>
</p>

---

## Why Timeforge?

| Pain | Timeforge |
|------|-----------|
| `git blame` is dry | **Blame+** with ownership bars |
| “Who broke CI?” | **Why** ranks likely culprit commits |
| Bus factor unknown | **Heatmap** + warning |
| PR touches one file… | **Blast radius** shows what usually moves with it |

---

## 60-second demo

```bash
git clone https://github.com/FounderB/Timeforge.git
cd Timeforge
cargo build --release

# run inside any git repo (or pass --repo)
./target/release/timeforge timeline README.md
./target/release/timeforge why --query fix
./target/release/timeforge heatmap
./target/release/timeforge blast src/main.rs

# cinematic UI
./target/release/timeforge serve
# → http://127.0.0.1:8790
```

---

## CLI

```bash
timeforge timeline <path> [--limit 30] [--json]
timeforge blame <path> [--lines 40] [--json]
timeforge why [--path src/] [--since "90 days ago"] [--query keyword] [--json]
timeforge heatmap [--since "180 days ago"] [--path src/] [--json]
timeforge blast <path> [--limit 15] [--json]
timeforge serve [--addr 127.0.0.1:8790]
```

Global: `--repo /path/to/git/repo`

---

## Features (v0.1)

1. **Time Machine** — file commit cinema (dates, +/- stats, PR `#` hints)
2. **Why broke** — scores recent commits (hotfix keywords, sensitive paths, blast size)
3. **Ownership heatmap** — commits/files per author + bus-factor warning
4. **Blast radius** — co-change affinity with the target path
5. **Web UI** — same engines in the browser

---

## Roadmap

- [x] CLI + local web demo
- [ ] GitHub App (PR check: blast radius)
- [ ] Guilty commit ↔ failing test deep link
- [ ] Diff cinema animation
- [ ] VS Code / Cursor extension

---

## License

MIT — see [LICENSE](LICENSE)
