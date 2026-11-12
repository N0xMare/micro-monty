from collections import defaultdict

deployments = await list_deployments({})
totals = defaultdict(int)
for deployment in deployments:
    totals[deployment["author"]] += deployment["duration_s"]
await submit_answer({"totals": {author: f"{seconds // 3600}:{seconds // 60 % 60:02d}:{seconds % 60:02d}" for author, seconds in totals.items()}})
