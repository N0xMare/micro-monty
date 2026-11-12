import json
from datetime import datetime

issues, page = [], json.loads(await search_issues({"query": "", "page": 1}))
issues.extend(page["issues"])
while page["has_more"]:
    page = json.loads(await search_issues({"query": "", "page": page["page"] + 1}))
    issues.extend(page["issues"])
keys = sorted(i["key"] for i in issues if datetime.fromisoformat(i["created"].replace("Z", "+00:00")).weekday() >= 5)
await submit_answer({"keys": keys})
