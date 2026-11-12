import json
from datetime import datetime, timezone

issues = []
page = 1
while True:
    result = json.loads(await search_issues({"query": "", "page": page}))
    issues.extend(issue for issue in result["issues"] if issue["status"] != "closed")
    if not result["has_more"]:
        break
    page += 1
counts = {f"P{priority}": sum(issue["priority"] == f"P{priority}" for issue in issues) for priority in range(1, 5)}
oldest = min(issues, key=lambda issue: (issue["created"], issue["key"]))
created = datetime.fromisoformat(oldest["created"].replace("Z", "+00:00"))
now = datetime(2026, 3, 15, 12, 0, tzinfo=timezone.utc)
days = (now - created).days
text = "\n".join([
    f"Open issues: {len(issues)}",
    *(f"  P{priority}: {counts[f'P{priority}']}" for priority in range(1, 5)),
    f"Oldest: {oldest['key']} ({days} days) - {oldest['title']}",
])
await post_message({"channel": "#eng", "text": text})
