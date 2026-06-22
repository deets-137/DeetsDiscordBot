# Copy to config.py and edit. config.py is gitignored.
#
# The bot token itself lives in .env as DISCORD_TOKEN (not here), matching
# DeetsCode. This file is for non-secret settings.

# Default channel the one-off script / harness scans when none is given.
# Discord developer mode → right-click channel → Copy ID.
DEFAULT_CHANNEL_ID = 0

# Channel IDs the slash command is allowed to run in. Empty set = allow anywhere.
ALLOWED_CHANNEL_IDS: set[int] = set()

# Device-side harness bind address. Keep on localhost — there's no auth.
HARNESS_HOST = "127.0.0.1"
HARNESS_PORT = 8077
