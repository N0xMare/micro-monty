from collections import defaultdict, Counter
from datetime import datetime, timezone

deployments = await list_deployments({})
durations = defaultdict(list)
days = Counter()
for deployment in deployments:
    durations[deployment["service"]].append(deployment["duration_s"])
    days[datetime.fromtimestamp(deployment["started_at"], timezone.utc).date().isoformat()] += 1
await submit_answer({"avg_minutes": {s: round(sum(v) / len(v) / 60, 1) for s, v in durations.items()}, "busiest_day": min(days, key=lambda d: (-days[d], d))})
