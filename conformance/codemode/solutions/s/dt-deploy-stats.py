from datetime import datetime, timezone
from collections import defaultdict, Counter

deps = await list_deployments({})
durs = defaultdict(list)
days = Counter()
for d in deps:
    durs[d["service"]].append(d["duration_s"])
    days[datetime.fromtimestamp(d["started_at"], tz=timezone.utc).date().isoformat()] += 1

avg_minutes = {s: round(sum(v) / len(v) / 60, 1) for s, v in durs.items()}
top = max(days.values())
busiest_day = min(day for day, n in days.items() if n == top)
print(len(deps), avg_minutes, busiest_day, top)
print(await submit_answer({"avg_minutes": avg_minutes, "busiest_day": busiest_day}))
