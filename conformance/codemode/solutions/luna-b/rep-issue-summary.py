import asyncio, json
from datetime import datetime, timezone

async def page(n):
    return json.loads(await search_issues({"query": "", "page": n}))

pages = [await page(1)]
for number in range(2, pages[0]["page"] + 1):
    pages.append(await page(number))
issues = [issue for response in pages for issue in response["issues"] if issue["status"] != "closed"]
counts = {f"P{i}": sum(issue["priority"] == f"P{i}" for issue in issues) for i in range(1, 5)}
oldest = min(issues, key=lambda issue: (issue["created"], issue["key"]))
created = datetime.fromisoformat(oldest["created"].replace("Z", "+00:00"))
now = datetime(2026, 3, 15, 12, 0, tzinfo=timezone.utc)
days = (now - created).days
text = (f"Open issues: {len(issues)}\n  P1: {counts['P1']}\n  P2: {counts['P2']}\n"
        f"  P3: {counts['P3']}\n  P4: {counts['P4']}\nOldest: {oldest['key']} ({days} days) - {oldest['title']}")
await post_message({"channel": "#eng", "text": text})
