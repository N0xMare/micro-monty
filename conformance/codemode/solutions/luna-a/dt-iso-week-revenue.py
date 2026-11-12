import json
from collections import defaultdict
from datetime import date
rows = json.loads(await query_database({"sql": "SELECT order_date,total FROM orders WHERE status='completed' AND order_date >= '2026-01-01' AND order_date < '2026-04-01'"}))
totals = defaultdict(float)
for row in rows:
    iso = date.fromisoformat(row["order_date"]).isocalendar()
    totals[f"{iso.year}-W{iso.week:02d}"] += row["total"]
await submit_answer({"weeks": {k: round(v, 2) for k, v in sorted(totals.items())}})
