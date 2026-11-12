from collections import Counter

counts = Counter()
token = None
while True:
    args = {"service": "auth", "level": "WARN"}
    if token:
        args["page_token"] = token
    resp = await query_logs(args)
    for e in resp["entries"]:
        counts[e["level"]] += 1
    token = resp.get("next_page_token")
    if not token:
        break
print(dict(counts))
await submit_answer({"counts": dict(counts)})
