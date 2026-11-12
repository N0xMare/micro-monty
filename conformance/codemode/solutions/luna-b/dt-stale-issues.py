import json
from datetime import datetime, timezone

issues, page = [], json.loads(await search_issues({"query": "", "page": 1}))
issues.extend(page["issues"])
while page["has_more"]:
    page = json.loads(await search_issues({"query": "", "page": page["page"] + 1}))
    issues.extend(page["issues"])
cutoff = datetime(2026, 3, 1, 12, 0, tzinfo=timezone.utc)
keys = sorted(i["key"] for i in issues if i["status"] != "closed" and datetime.fromisoformat(i["updated"].replace("Z", "+00:00")) < cutoff)
await submit_answer({"keys": keys})
