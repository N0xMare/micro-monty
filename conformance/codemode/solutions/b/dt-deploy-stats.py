import json
from collections import Counter, defaultdict
from datetime import datetime, timezone

deps = json.loads(await list_deployments({}))
print(len(deps))

durations = defaultdict(list)
days = Counter()
for d in deps:
    durations[d["service"]].append(d["duration_s"])
    day = datetime.fromtimestamp(d["started_at"], tz=timezone.utc).date().isoformat()
    days[day] += 1

avg_minutes = {s: round(sum(v) / len(v) / 60, 1) for s, v in durations.items()}
max_count = max(days.values())
busiest_day = min(day for day, n in days.items() if n == max_count)

answer = {"avg_minutes": avg_minutes, "busiest_day": busiest_day}
print(answer, max_count)
print(await submit_answer(answer))
