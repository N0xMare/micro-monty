import json
from collections import Counter, defaultdict
from datetime import datetime, timezone

rows = json.loads(await list_deployments({}))
totals, counts, days = defaultdict(int), Counter(), Counter()
for d in rows:
    totals[d["service"]] += d["duration_s"]
    counts[d["service"]] += 1
    days[datetime.fromtimestamp(d["started_at"], timezone.utc).date().isoformat()] += 1
averages = {s: round(totals[s] / counts[s] / 60, 1) for s in totals}
busiest = min((day for day, n in days.items() if n == max(days.values())))
await submit_answer({"avg_minutes": averages, "busiest_day": busiest})
