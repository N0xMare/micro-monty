import json
from collections import Counter

counts = Counter()
token = None
while True:
    args = {"service": "auth", "level": "WARN"}
    if token:
        args["page_token"] = token
    page = json.loads(await query_logs(args))
    for e in page["entries"]:
        counts[e["level"].strip()] += 1
    token = page.get("next_page_token")
    if not token:
        break

counts = dict(counts)
print(counts)
print(await submit_answer({"counts": counts}))
