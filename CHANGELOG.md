## 0.4.2 — Update/Repair always in-place

- `update` / `repair` / `open --repair` never wipe-and-reclone into a new folder
- Same path only: `git fetch` + materialize missing objects; local files survive
- `UpdateResult.same_path` + `mode` (`inplace-fetch` / `inplace-materialize`)
- Tests: local bare remotes prove new files appear without path change

## 0.4.1 — Promisor harden + pin × fix

- Pin unpin works (× is a real button; stopPropagation)
- `git_in` retries with network lazy-fetch when objects missing
- Partial clones no longer force-broken by harden; Update materializes objects
- `timeforge repair` / `/api/repair` re-clones current repo from origin
- UI: partial-clone banner + error CTAs (Update / Repair)
- Open-with-Update uses network fetch correctly

## 0.4.0 — Bug Hunt · Archaeology · Update

- **Bug Hunt / Radar** (`hunt` / `radar`) — suspects + dig + fix↔break + ghosts in one shot
- **Archaeology** (`dig <pattern>`) — `git log -S` first/last seen for code patterns
- **Fix↔Break** (`pairs`) — fix commits paired with likely earlier culprits
- **Update** (`update` + UI button) — `git fetch` + ff-pull current repo
- Hunt scoring hardened: garbage keywords no longer spam weak suspects
- UI: Bug Hunt default tab, Archaeology, Fix↔Break, Update / `u` key

## 0.3.0 — Blame Map · PR Travel · Ghosts

- **Blame Map** (`map` / `/api/map`) — ownership zones + color strip
- **PR Time Travel** (`pr #N`) — files in a PR + later touches on same paths
- **Ghost authors** (`ghosts`) — silent owners + path bus-factor risks
- Timeline is rename-aware (`git log --follow`)
- Tree shows churn sparks; pins + keyboard (`j/k`, `b`, `p`, `g`) + deep links

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
