import json
from collections import defaultdict
items = json.loads(await list_deployments({})); totals = defaultdict(int)
for x in items: totals[x["author"]] += x["duration_s"]
formatted = {}
for author, n in totals.items():
    h, rem = divmod(n, 3600); m, s = divmod(rem, 60)
    formatted[author] = f"{h}:{m:02d}:{s:02d}"
await submit_answer({"totals": formatted})
