## 0.6.3 — Unlock hunt + bind guard

- `/api/hunt` (and other git-heavy routes) clone `Repo` under the lock then unlock before long git work
- Refuse non-loopback `serve --addr` without `--expose` or `--token` / `TIMEFORGE_TOKEN`
- `--repair` help text matches in-place repair (never wipe-reclone)
- README version badge → 0.6.3

## 0.6.2 — Fix UI stuck on Loading

- `/api/info` no longer holds the repo lock during `ls-remote` (blocked the whole UI)
- Boot: `info?fast=1` + tree + cached in parallel; network pill loads in background
- Harder tree/info error surfacing so the page never sits on “Loading…” forever

## 0.6.1 — Remove cached repos

- `timeforge repos rm owner/repo --yes` / `repos clean --yes`
- UI: × on cached chips deletes `~/.timeforge/repos/…` only (confirm)
- API: `POST /api/repos` with `{spec|id}` or `{clean:true}`

## 0.6.0 — Story Ask + speed

- Parse Ask into path / `file:line` / `::symbol` / dig needle / PR (`parsed` on report)
- Path-scoped dig + blame; stack-like queries drive the hunt
- Speed: skip pairs for pure code tokens; skip `-G` fallback in fast dig; tighter fix↔break
- PR answers include later-touch hits + “After this PR” drill-down
- Golden calibration tests (local fixtures + optional cached remotes)

## 0.5.4 — Calibration pass

- CLI: `--json` after ask query no longer swallowed into the question
- `#N` miss returns `pr-miss` (no fake dig on `#1`)
- Dig stopwords (`fix`, `doc`, …) — don't treat English as code tokens
- Fix↔break: skip typo/doc noise; reject HTML/roff pickaxe needles
- Answer ranking: dig vs pickaxe vs blame by evidence×confidence (blame needs query match)

## 0.5.3 — Ask answers with proof

- ANSWER carries `evidence` (proven/strong/heuristic/weak), `method`, `confidence`, drill-downs
- Prefer pickaxe/blame fix↔break and dig first-seen over subject heuristics
- Weak co-change pairs never become the Ask answer
- UI: Ask / History / Blame primary; power tools demoted; evidence badges + drill buttons

## 0.5.2 — Causality over keywords

- Safe `git log` delimiter (`\\x1f`) — subjects with `|` no longer break parsing
- Shared `parse_log_line` / `parse_name_only_log` across modules
- **why**: fix/bug/wip-in-subject ≠ culprit; repair commits down-ranked
- **fix↔break**: pickaxe on removed hunks + blame of pre-fix lines (co-change is weak fallback)
- CLI Ask-first: `timeforge ask …` / bare `timeforge "…"`; power tools hidden from help

## 0.5.1 — Brand + dark scrollbars

- Official icon (`assets/timeforge-icon.png`) in UI, favicon, README
- Dark teal scrollbars (no white tracks)
- README relaunch: Ask-first, badges, mermaid

## 0.5.0 — Fast one-question hunt

- Parallel hunt (why ∥ dig ∥ pairs); skips heavy ghosts/blame unless path given
- PR numbers (`#12`) take a dedicated fast path
- `answer` + `elapsed_ms` on every hunt; `/api/ask`
- Network probe in `/api/info` (online/offline pill)
- UI Ask box — one question, Enter to hunt; tree loads without churn by default

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
