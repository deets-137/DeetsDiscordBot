"""
Device-side harness for DeetsOTD.

A tiny local FastAPI app (same stack as DeetsCode's server.py) that lets you
trigger a channel scan from your own machine — a browser form or a curl/script
call — without going through Discord. Each scan reuses scan.py's scan_channel,
which logs in, fetches, writes exports/, and logs out.

Run:
  pip install -r requirements.txt
  python harness.py
  open http://127.0.0.1:8077

Trigger from a script:
  curl -X POST 127.0.0.1:8077/scan -H 'content-type: application/json' \
       -d '{"channel_id": 123456789012345678, "limit": 500}'

Localhost only, no auth — do not expose it.
"""

from pathlib import Path

import uvicorn
from fastapi import FastAPI
from fastapi.responses import HTMLResponse, JSONResponse
from pydantic import BaseModel

from scan import scan_channel

try:
    import config
except ModuleNotFoundError:
    raise SystemExit("No config.py — copy config.example.py to config.py first.")

app = FastAPI(title="DeetsOTD harness")


class ScanRequest(BaseModel):
    channel_id: int | None = None
    limit: int | None = None


_PAGE = """<!doctype html><html><head><meta charset=utf-8>
<title>DeetsOTD</title>
<style>
 body{font:15px system-ui;max-width:34rem;margin:3rem auto;padding:0 1rem}
 input{font:inherit;padding:.5rem;width:100%;box-sizing:border-box;margin:.3rem 0}
 button{font:inherit;padding:.5rem 1rem;cursor:pointer;margin-top:.5rem}
 pre{background:#f4f4f4;padding:1rem;border-radius:6px;white-space:pre-wrap}
</style></head><body>
<h1>DeetsOTD link scan</h1>
<label>Channel ID <input id=cid placeholder="%(default)s" value="%(default)s"></label>
<label>Limit (blank = all) <input id=lim type=number></label>
<button onclick=go()>Scan</button>
<pre id=out>ready.</pre>
<script>
async function go(){
  const out=document.getElementById('out'); out.textContent='scanning…';
  const body={channel_id:Number(document.getElementById('cid').value)||null,
              limit:Number(document.getElementById('lim').value)||null};
  const r=await fetch('/scan',{method:'POST',headers:{'content-type':'application/json'},
                              body:JSON.stringify(body)});
  out.textContent=JSON.stringify(await r.json(),null,2);
}
</script></body></html>"""


@app.get("/", response_class=HTMLResponse)
def index():
    return _PAGE % {"default": config.DEFAULT_CHANNEL_ID or ""}


@app.post("/scan")
async def scan(req: ScanRequest):
    channel_id = req.channel_id or config.DEFAULT_CHANNEL_ID
    if not channel_id:
        return JSONResponse({"error": "no channel_id and DEFAULT_CHANNEL_ID is 0"},
                            status_code=400)
    try:
        result = await scan_channel(channel_id, req.limit)
    except Exception as e:  # surface login / permission / fetch errors to the caller
        return JSONResponse({"error": str(e)}, status_code=500)
    return {
        "channel": result.get("label"),
        "count": result.get("count"),
        "markdown": str(result.get("md")),
        "csv": str(result.get("csv")),
    }


if __name__ == "__main__":
    uvicorn.run(app, host=config.HARNESS_HOST, port=config.HARNESS_PORT)
