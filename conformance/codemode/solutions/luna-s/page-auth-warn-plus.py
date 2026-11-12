from collections import Counter

counts = Counter()
token = None
while True:
    args = {"service": "auth", "level": "WARN"}
    if token is not None:
        args["page_token"] = token
    page = await query_logs(args)
    counts.update(entry["level"] for entry in page["entries"])
    token = page["next_page_token"]
    if token is None:
        break
await submit_answer({"counts": dict(counts)})
