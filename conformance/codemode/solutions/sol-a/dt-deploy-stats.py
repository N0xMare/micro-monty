import json
from collections import Counter, defaultdict
from datetime import datetime, timezone

deployments = json.loads(await list_deployments({}))
durations = defaultdict(list)
days = Counter()
for d in deployments:
    durations[d["service"]].append(d["duration_s"])
    days[datetime.fromtimestamp(d["started_at"], timezone.utc).date().isoformat()] += 1
await submit_answer({"avg_minutes": {s: round(sum(values) / len(values) / 60, 1) for s, values in durations.items()}, "busiest_day": min(days, key=lambda day: (-days[day], day))})
