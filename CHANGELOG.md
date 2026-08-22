## 0.2.1 — File explorer + promisor fix

- Web UI: file tree with icons, breadcrumbs, click folders/files
- `timeforge tree [path]` + `/api/tree`
- Full clones by default; `open --repair` / `--full` fixes broken partial/promisor caches
- Offline-safe git (`GIT_NO_LAZY_FETCH`); timeline falls back without `--numstat`
- UI: Repair clone button; click hotspots/stale/blast → timeline

## 0.2.0 — Remote repos + more insights

- `timeforge open owner/repo` — clone/cache any public GitHub repo to `~/.timeforge/repos`
- `--repo owner/repo|URL|path` on every command
- `repos` — list cache
- New: `hotspots`, `stale`, `contributors`, `churn`
- Web UI: open remote from browser + new tabs

## 0.1.0 — Initial release

- `timeline` — Repo Time Machine for a file
- `blame` — Blame+ with ownership stats
- `why` — rank likely culprit commits
- `heatmap` — ownership / bus factor
- `blast` — co-change blast radius
- `serve` — local web UI on :8790
