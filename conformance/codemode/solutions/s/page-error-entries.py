errors = []
token = None
while True:
    args = {"service": "gateway", "level": "ERROR"}
    if token:
        args["page_token"] = token
    resp = await query_logs(args)
    errors.extend(e for e in resp["entries"] if e["level"] == "ERROR")
    token = resp.get("next_page_token")
    if not token:
        break
last_ts = max(e["ts"] for e in errors) if errors else ""
print(len(errors), last_ts)
await submit_answer({"count": len(errors), "last_ts": last_ts})
