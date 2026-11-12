import datetime
from collections import Counter, defaultdict

deployments = await list_deployments({})
durations = defaultdict(list)
days = Counter()
for deployment in deployments:
    durations[deployment["service"]].append(int(deployment["duration_s"]) / 60)
    day = datetime.datetime.fromtimestamp(int(deployment["started_at"]), datetime.timezone.utc).date().isoformat()
    days[day] += 1
avg_minutes = {service: round(sum(values) / len(values), 1) for service, values in durations.items()}
busiest_day = min(day for day, count in days.items() if count == max(days.values()))
await submit_answer({"avg_minutes": avg_minutes, "busiest_day": busiest_day})
