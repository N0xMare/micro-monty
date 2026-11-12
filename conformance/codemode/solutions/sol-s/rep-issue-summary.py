from collections import Counter
from datetime import datetime

issues = []
page = 1
while True:
    result = await search_issues({"query": "", "page": page})
    issues.extend(i for i in result["issues"] if i["status"] != "closed")
    if not result["has_more"]:
        break
    page += 1
def created(issue):
    return datetime.fromisoformat(issue["created"].replace("Z", "+00:00"))
oldest = min(issues, key=lambda i: (created(i), i["key"]))
days = (datetime.fromisoformat("2026-03-15T12:00:00+00:00") - created(oldest)).days
counts = Counter(i["priority"] for i in issues)
lines = [f"Open issues: {len(issues)}"]
lines.extend(f"  P{p}: {counts[f'P{p}']}" for p in range(1, 5))
lines.append(f"Oldest: {oldest['key']} ({days} days) - {oldest['title']}")
await post_message({"channel": "#eng", "text": "\n".join(lines)})
