import json
from collections import Counter

counts = Counter()
token = None
while True:
    arguments = {"service": "auth", "level": "WARN"}
    if token is not None:
        arguments["page_token"] = token
    page = json.loads(await query_logs(arguments))
    counts.update(entry["level"] for entry in page["entries"])
    token = page["next_page_token"]
    if token is None:
        break
await submit_answer({"counts": dict(counts)})
