import json
from collections import Counter
from datetime import datetime

issues = []
page = 1
while True:
    data = json.loads(await search_issues({"query": "", "page": page}))
    issues.extend(i for i in data["issues"] if i["status"] != "closed")
    if not data["has_more"]:
        break
    page += 1
counts = Counter(i["priority"] for i in issues)
oldest = min(issues, key=lambda i: (datetime.fromisoformat(i["created"]), i["key"]))
days = (datetime.fromisoformat("2026-03-15T12:00:00Z") - datetime.fromisoformat(oldest["created"])).days
lines = [f"Open issues: {len(issues)}"]
lines.extend(f"  {priority}: {counts[priority]}" for priority in ["P1", "P2", "P3", "P4"])
lines.append(f"Oldest: {oldest['key']} ({days} days) - {oldest['title']}")
await post_message({"channel": "#eng", "text": "\n".join(lines)})
