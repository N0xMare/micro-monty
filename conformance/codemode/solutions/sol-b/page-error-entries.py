import json

args = {"service": "gateway", "level": "ERROR"}
count = 0
last_ts = ""
while True:
    page = json.loads(await query_logs(args))
    count += len(page["entries"])
    if page["entries"]:
        last_ts = page["entries"][-1]["ts"]
    if page["next_page_token"] is None:
        break
    args = {"service": "gateway", "level": "ERROR", "page_token": page["next_page_token"]}
await submit_answer({"count": count, "last_ts": last_ts})
