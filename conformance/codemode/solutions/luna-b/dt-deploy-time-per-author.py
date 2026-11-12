import json
from collections import defaultdict

rows = json.loads(await list_deployments({}))
totals = defaultdict(int)
for d in rows:
    totals[d["author"]] += d["duration_s"]
formatted = {author: f"{seconds // 3600}:{(seconds % 3600) // 60:02d}:{seconds % 60:02d}" for author, seconds in totals.items()}
await submit_answer({"totals": formatted})
