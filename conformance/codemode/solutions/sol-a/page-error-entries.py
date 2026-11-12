import json

args = {"service": "gateway", "level": "ERROR"}
count = 0
last_ts = None
while True:
    page = json.loads(await query_logs(args))
    count += len(page["entries"])
    if page["entries"]:
        last_ts = page["entries"][-1]["ts"]
    if page["next_page_token"] is None:
        break
    args["page_token"] = page["next_page_token"]
if last_ts is None:
    raise ValueError("No ERROR entries exist")
await submit_answer({"count": count, "last_ts": last_ts})
