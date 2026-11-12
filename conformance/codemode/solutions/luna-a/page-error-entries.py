import json

token = None
entries = []
while True:
    args = {"service": "gateway", "level": "ERROR"}
    if token is not None:
        args["page_token"] = token
    page = json.loads(await query_logs(args))
    entries.extend(page["entries"])
    token = page["next_page_token"]
    if token is None:
        break
await submit_answer({"count": len(entries), "last_ts": entries[-1]["ts"]})
