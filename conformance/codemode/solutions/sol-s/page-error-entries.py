args = {"service": "gateway", "level": "ERROR"}
count = 0
last_ts = None
while True:
    result = await query_logs(args)
    count += len(result["entries"])
    if result["entries"]:
        last_ts = result["entries"][-1]["ts"]
    if result["next_page_token"] is None:
        break
    args["page_token"] = result["next_page_token"]
if last_ts is None:
    raise RuntimeError("No ERROR entries; no newest timestamp exists")
await submit_answer({"count": count, "last_ts": last_ts})
