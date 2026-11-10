---
status: idea
desk_test: none
sources: []
updated: 2026-11-10
---
# Rendered image cards

## 1. What he said (2026-11-10)
"Let's build it, we'll try image cards after I see how native cards look." Then: "native
cards look great."

## 2. Claude's notes
The bot draws a PNG of the np card and the queue page in DeetsMusic's own fonts and theme
colours, with ◀ ▶ buttons under it. It looks most like the app. Costs: rows are not
clickable, each page is a new upload, and it is less sharp on a phone. A fair test: the same
queue page both ways, side by side, on desktop and phone. Fonts come from DeetsMusic's
`src/styles/fonts/` (check their licenses before shipping them here).
