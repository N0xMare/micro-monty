from collections import defaultdict

deployments = await list_deployments({})
totals = defaultdict(int)
for deployment in deployments:
    totals[deployment["author"]] += int(deployment["duration_s"])
formatted = {}
for author, seconds in totals.items():
    hours, remainder = divmod(seconds, 3600)
    minutes, seconds = divmod(remainder, 60)
    formatted[author] = f"{hours}:{minutes:02d}:{seconds:02d}"
await submit_answer({"totals": formatted})
