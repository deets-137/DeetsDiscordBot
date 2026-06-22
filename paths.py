"""Single source of truth for filesystem paths (mirrors DeetsCode's paths.py)."""

from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent
EXPORTS_DIR = PROJECT_ROOT / "exports"
DB_PATH = PROJECT_ROOT / "storage.db"
