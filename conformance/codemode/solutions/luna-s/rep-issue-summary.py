from datetime import datetime, timezone
issues = []
page = 1
while True:
    result = await search_issues({"query": "", "page": page})
    issues.extend(result["issues"])
    if not result["has_more"]:
        break
    page += 1
active = [x for x in issues if x["status"] != "closed"]
counts = {f"P{i}": sum(1 for x in active if x["priority"] == f"P{i}") for i in range(1, 5)}
oldest = min(active, key=lambda x: (x["created"], x["key"]))
now = datetime(2026, 3, 15, 12, tzinfo=timezone.utc)
created = datetime.fromisoformat(oldest["created"].replace("Z", "+00:00"))
days = (now - created).days
text = f"Open issues: {len(active)}\n  P1: {counts['P1']}\n  P2: {counts['P2']}\n  P3: {counts['P3']}\n  P4: {counts['P4']}\nOldest: {oldest['key']} ({days} days) - {oldest['title']}"
await post_message({"channel": "#eng", "text": text})
