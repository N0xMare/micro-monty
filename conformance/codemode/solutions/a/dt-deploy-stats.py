import json
from collections import Counter, defaultdict
from datetime import datetime, timezone

deps = json.loads(await list_deployments({}))

durs = defaultdict(list)
days = Counter()
for d in deps:
    durs[d["service"]].append(d["duration_s"])
    days[datetime.fromtimestamp(d["started_at"], tz=timezone.utc).date().isoformat()] += 1

avg_minutes = {s: round(sum(v) / len(v) / 60, 1) for s, v in sorted(durs.items())}
max_count = max(days.values())
busiest_day = min(day for day, c in days.items() if c == max_count)
print(len(deps), avg_minutes, busiest_day, max_count)
print(await submit_answer({"avg_minutes": avg_minutes, "busiest_day": busiest_day}))
