---
status: project
desk_test: open
sources: [journal/scan.py, journal/letterboxd_web.py]
updated: 2026-11-10
---
# The nightly publish, and the cut-over from DeetsOTD

> **Status:** project · 2026-11-10. The nightly job still runs from `../DeetsOTD`. This repo
> passed the side-by-side test (§3). The switch (§4) is next.

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

## 3. The side-by-side test (passed 2026-11-10)
1. Copy `storage.db` with the backup API (architecture/STORAGE.md §5).
2. Export from both repos and compare, minus `generated_at`:
   songs 483 = 483, hash `f203764832f45ecc` both; movies 322 = 322, hash `fbf7364479063367` both.
3. Live from this repo, to a scratch folder: the scan found 1 new post (484), the RSS run 1 new
   film (323), both exit 0.

## 4. The cut-over checklist
1. [ ] After a 6 PM run has finished, stop and copy DeetsOTD's `storage.db` here again (it took
   tonight's posts), and `exports/tmdb_posters.json`. Compare table counts.
2. [ ] Edit `nightly-sotd.ps1` (§2) and run it by hand with the commit/push step off; the JSON
   must match what DeetsOTD just published.
3. [ ] Turn the commit/push step back on. Let one real 6 PM run pass; read `nightly-sotd.log`.
4. [ ] Update the other DeetsSolutions references (§2).
5. [ ] DeetsOTD: a README pointer here; merge its `lil-deets-voice` branch; push; archive the
   GitHub repo (read-only). Keep the local folder until he says.
