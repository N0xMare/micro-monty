import json
from collections import defaultdict

totals = defaultdict(int)
for deployment in json.loads(await list_deployments({})):
    totals[deployment["author"]] += deployment["duration_s"]
await submit_answer({"totals": {author: f"{seconds // 3600}:{seconds % 3600 // 60:02d}:{seconds % 60:02d}" for author, seconds in totals.items()}})
