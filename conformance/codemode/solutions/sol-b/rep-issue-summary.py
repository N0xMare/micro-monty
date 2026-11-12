import asyncio, json
from datetime import datetime
from collections import Counter

async def fetch_status(status):
    issues = []
    page = 1
    while True:
        data = json.loads(await search_issues({"query": f"status:{status}", "page": page}))
        issues.extend(data["issues"])
        if not data["has_more"]:
            return issues
        page += 1

groups = await asyncio.gather(fetch_status("open"), fetch_status("in_progress"))
issues = [i for group in groups for i in group]
counts = Counter(i["priority"] for i in issues)
oldest = min(issues, key=lambda i: (datetime.fromisoformat(i["created"].replace("Z", "+00:00")), i["key"]))
days = (datetime.fromisoformat("2026-03-15T12:00:00+00:00") - datetime.fromisoformat(oldest["created"].replace("Z", "+00:00"))).days
lines = [f"Open issues: {len(issues)}"] + [f"  {p}: {counts[p]}" for p in ["P1", "P2", "P3", "P4"]]
lines.append(f"Oldest: {oldest['key']} ({days} days) - {oldest['title']}")
await post_message({"channel": "#eng", "text": "\n".join(lines)})
