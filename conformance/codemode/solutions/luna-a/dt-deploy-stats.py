import json
from collections import Counter, defaultdict
from datetime import datetime, timezone
items = json.loads(await list_deployments({}))
durations, days = defaultdict(list), Counter()
for x in items:
    durations[x["service"]].append(x["duration_s"] / 60)
    days[datetime.fromtimestamp(x["started_at"], timezone.utc).date().isoformat()] += 1
await submit_answer({"avg_minutes": {s: round(sum(v)/len(v), 1) for s, v in durations.items()},
                     "busiest_day": min(days, key=lambda d: (-days[d], d))})
