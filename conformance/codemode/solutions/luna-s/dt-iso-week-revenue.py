import datetime
from collections import defaultdict

rows = await query_database({"sql": "SELECT order_date, total FROM orders WHERE status = 'completed' AND order_date >= '2026-01-01' AND order_date < '2026-04-01'"})
totals = defaultdict(float)
for row in rows:
    date = datetime.date.fromisoformat(row["order_date"])
    iso = date.isocalendar()
    totals[f"{iso.year}-W{iso.week:02d}"] += float(row["total"])
await submit_answer({"weeks": {week: round(total, 2) for week, total in sorted(totals.items())}})
