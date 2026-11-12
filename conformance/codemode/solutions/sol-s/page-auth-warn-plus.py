counts = {"WARN": 0, "ERROR": 0}
args = {"service": "auth", "level": "WARN"}
while True:
    result = await query_logs(args)
    for entry in result["entries"]:
        counts[entry["level"]] += 1
    if result["next_page_token"] is None:
        break
    args["page_token"] = result["next_page_token"]
await submit_answer({"counts": counts})
