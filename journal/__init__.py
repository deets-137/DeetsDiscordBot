"""The Song-of-the-Day and film journals: read a Discord channel, resolve the songs, keep
them in storage.db, and export what deets.solutions shows (songs.json, movies.json).

Run from the repo root as modules: `python -m journal.scan …`, `python -m journal.review …`,
`python -m journal.letterboxd_web …`. docs/features/JOURNAL.md is the map.

`config.py` (non-secret settings) sits at the repo root beside `.env`; the root goes on
sys.path here so `import config` works whatever the working directory is.
"""

import sys

from .paths import PROJECT_ROOT

if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))
