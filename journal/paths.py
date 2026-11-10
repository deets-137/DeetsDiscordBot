"""Single source of truth for filesystem paths (mirrors DeetsCode's paths.py).

Everything private or generated lives at the repo root, beside `bot/` and `journal/`:
`.env`, `config.py`, `storage.db`, `exports/`, `secrets/` (all gitignored).
"""

from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent  # the repo root; this file is journal/paths.py
EXPORTS_DIR = PROJECT_ROOT / "exports"
DB_PATH = PROJECT_ROOT / "storage.db"
