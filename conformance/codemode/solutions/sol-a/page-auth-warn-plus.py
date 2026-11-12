import json

args = {"service": "auth", "level": "WARN"}
counts = {"WARN": 0, "ERROR": 0}
while True:
    page = json.loads(await query_logs(args))
    for entry in page["entries"]:
        counts[entry["level"]] = counts.get(entry["level"], 0) + 1
    if page["next_page_token"] is None:
        break
    args["page_token"] = page["next_page_token"]
await submit_answer({"counts": counts})
