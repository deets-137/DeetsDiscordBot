---
status: sop
desk_test: none
sources: []
updated: 2026-11-10
---
# How the docs are organized

A short copy of DeetsMusic's rules, as DeetsPics keeps them. The long form, with the reasons,
is `../DeetsMusic/docs/DOCS-ORG.md`. Where the two differ, this file holds here.

## 1. The tree
Docs sit in folders by area, not by state.

| Folder | Holds |
|---|---|
| `docs/` | DESIGN, HANDOFF, WORKLOG, LESSONS, this file |
| `docs/architecture/` | the engine the commands run on |
| `docs/features/` | one command group, one surface, or one pipeline |
| `docs/ops/` | set up, build, run, diagnose |
| `docs/guide/` | written for the user |
| `docs/integrations/` | another service's API as this repo uses it |
| `docs/ideas/` | not built; never tell a user the bot does these |
| `docs/archive/` | docs no longer needed, kept as a record |

A folder is made when its first doc is written.

## 2. Front matter
Every doc starts with this block, above the `#` title.

```yaml
---
status: shipped | built | designed | project | parked | idea | foundation | sop | guide | archived
desk_test: passed 2026-11-10 | open | none
sources: [bot/dm.py, bot/dm_bridge.py]
updated: 2026-11-10
---
```

| value | meaning |
|---|---|
| `shipped` | in daily use on the desk (this repo has no installer yet) |
| `built` | in the tree, not yet used for real |
| `designed` | every fork closed, no code |
| `project` | started, not complete: forks still open, or a build begun |
| `parked` | designed and stopped on purpose; kept as a record |
| `idea` | not designed. Everything in `ideas/` |
| `foundation` | the engine the features run on |
| `sop` | a standard operating procedure: how the work is done |
| `guide` | written for the user |
| `archived` | no longer needed; in `docs/archive/` |

`sources` lists the code files the doc describes, so a reader can tell when a doc is stale.

## 3. Part markers
A doc with a built part and an unbuilt part gives, in its front matter, the state of the part
that works today. Every other part carries one line under its heading:

```
## 5. Vote to skip
> **Part:** idea · 2026-11-10
```

## 4. Whose words
In a doc that mixes the owner's decisions with Claude's proposals, say which is which, by
section. In an `ideas/` doc, §1 is what he said and is fixed.

## 5. HANDOFF and WORKLOG
- **A fact that is still true belongs in HANDOFF.** It is corrected in place.
- **A record of a day belongs in WORKLOG.** One dated `##` per sitting, newest first. An old
  entry is never corrected.
- What is open now is HANDOFF › Open now. There is no separate backlog doc.

## 6. State lives in each doc
Never in `CLAUDE.md`. `CLAUDE.md` gets one line per doc: its name, what it covers, the main
source file, and any rule a builder must keep.

## 7. No checker yet
DeetsMusic's `docs:check` is a Node script and this repo has no Node. Until a checker comes
over, check links and front matter by hand after a move.
