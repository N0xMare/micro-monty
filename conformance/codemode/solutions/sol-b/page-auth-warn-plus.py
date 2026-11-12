import json
from collections import Counter

args = {"service": "auth", "level": "WARN"}
counts = Counter()
while True:
    page = json.loads(await query_logs(args))
    counts.update(entry["level"] for entry in page["entries"])
    if page["next_page_token"] is None:
        break
    args = {"service": "auth", "level": "WARN", "page_token": page["next_page_token"]}
await submit_answer({"counts": dict(counts)})
