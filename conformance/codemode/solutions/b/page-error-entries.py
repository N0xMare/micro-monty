import json

errors = []
token = None
while True:
    args = {"service": "gateway", "level": "ERROR"}
    if token:
        args["page_token"] = token
    page = json.loads(await query_logs(args))
    errors.extend(e for e in page["entries"] if e["level"].strip() == "ERROR")
    token = page.get("next_page_token")
    if not token:
        break

count = len(errors)
last_ts = max(e["ts"] for e in errors) if errors else ""
print(count, last_ts)
print(await submit_answer({"count": count, "last_ts": last_ts}))
