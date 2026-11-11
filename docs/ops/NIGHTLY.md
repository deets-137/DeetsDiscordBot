---
status: project
desk_test: open
sources: [journal/scan.py, journal/letterboxd_web.py]
updated: 2026-11-10
---
# The nightly publish, and the cut-over from DeetsOTD

> **Status:** project · 2026-11-10. Switched at 14:40. The 6 PM run of 2026-11-10 was the first
> from this repo, and it passed. §4 is complete (2026-11-10): DeetsOTD is archived.

## 1. The job
Windows Task Scheduler, task **"DeetsOTD Nightly SOTD"**, every day at 6 PM, runs
`../DeetsSolutions/scripts/nightly-sotd.ps1` (registered by `register-nightly-sotd.ps1`):
1. Songs: in the journal repo, `scan <channel> --enrich --web --web-out <site>/sotd/songs.json`.
2. Movies: `letterboxd_web rss --web -o <site>/movies/movies.json`.
3. Projects: `build-project-dates.py` (GitHub last-commit dates for the site's project cards).
4. Commit and push DeetsSolutions only if a file's content changed (`generated_at` ignored).
The `journal-refresh` skill in DeetsSolutions runs the same script.

## 2. What changes at the switch (DeetsSolutions)
| File | Change |
|---|---|
| `scripts/nightly-sotd.ps1` | `$otd` → `…\DeetsDiscordBot`; `& $python "scan.py" …` → `& $python -m journal.scan …`; `"letterboxd_web.py"` → `-m journal.letterboxd_web`; comments and log text |
| `scripts/register-nightly-sotd.ps1` | comments only; the task keeps its name or is re-registered as "DeetsDiscordBot Nightly Journal" |
| `.claude/skills/journal-refresh/SKILL.md` | the repo name and the `review` command |
| `docs/data.md`, `docs/architecture.md`, `docs/bluesky.md`, `README.md`, `CLAUDE.md` | links and commands |
| `cool-stuff/` project card, `movies/`, `sotd/`, `styles/main.css` comments | the repo link (after the archive) |

**Other repos that read DeetsOTD's data:**
| Repo | Where | Change |
|---|---|---|
| `../DeetsSQL` | `src-tauri/src/db.rs` `default_db_path()` hardcodes `Documents\DeetsOTD\storage.db`; also its CLAUDE.md, README, `src/api.ts`, `src/main.ts`, `src/rail.ts` comments | point at `Documents\DeetsDiscordBot\storage.db` (or try it first, then DeetsOTD's). Its rule stays: read-only, three locks. |

## 3. The side-by-side test (passed 2026-11-10)
1. Copy `storage.db` with the backup API (architecture/STORAGE.md §5).
2. Export from both repos and compare, minus `generated_at`:
   songs 483 = 483, hash `f203764832f45ecc` both; movies 322 = 322, hash `fbf7364479063367` both.
3. Live from this repo, to a scratch folder: the scan found 1 new post (484), the RSS run 1 new
   film (323), both exit 0.

## 4. The cut-over checklist
1. [x] 2026-11-10 14:38: `storage.db` copied again from DeetsOTD (backup API; every table's
   count equal: 483 posts, 477 songs, 88 scans, 322 films, 213 watches, 87 syncs) and
   `exports/tmdb_posters.json` (same hash). DeetsOTD's copy is frozen from here.
2. [x] 2026-11-10 14:40: `nightly-sotd.ps1` runs `python -m journal.scan` /
   `-m journal.letterboxd_web` in this repo, with a `-Check` preflight (passed). DeetsSolutions
   `a935bba`, pushed. (No hand run: tonight's real run is the first test, by his choice.)
3. [x] 2026-11-10 18:00: the 6 PM run passed (`nightly-sotd.log`, "=== done" at 18:00:51).
   2 new posts, not the 1 expected: a second Song of the Day was posted after the 14:40 check.
   Published 485 songs and 323 films; one commit and push, DeetsSolutions `df170f1`.
   This repo's `storage.db`: 485 posts, 89 scans, 323 films. DeetsOTD's: 483 posts and 322 films,
   unchanged; its working tree is clean.
4. [x] 2026-11-10: DeetsSolutions references switched (docs, README, CLAUDE.md, the
   journal-refresh skill, page comments; the cool-stuff card is now **DeetsDiscordBot**, and
   `projects.json` was rebuilt). The scheduled task keeps its name "DeetsOTD Nightly SOTD".
   DeetsSQL's `default_db_path()` now opens `Documents\DeetsDiscordBot\storage.db` only: no
   fallback to DeetsOTD's frozen copy, which would show old data as current. `cargo check` and
   `tsc` pass. The installed DeetsSQL changes only after a rebuild. Pushed: DeetsSolutions
   `2542d4d`, DeetsSQL `61bf0fc`.
5. [x] 2026-11-10: DeetsOTD's README points here; `lil-deets-voice` fast-forwarded into main;
   pushed (`81bbf2c`); the GitHub repo is archived (read-only). The local folder stays until
   he says.
