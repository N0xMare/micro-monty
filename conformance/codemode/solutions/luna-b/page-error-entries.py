import json

entries = []
token = None
while True:
    arguments = {"service": "gateway", "level": "ERROR"}
    if token is not None:
        arguments["page_token"] = token
    page = json.loads(await query_logs(arguments))
    entries.extend(page["entries"])
    token = page["next_page_token"]
    if token is None:
        break
await submit_answer({"count": len(entries), "last_ts": entries[-1]["ts"] if entries else ""})
