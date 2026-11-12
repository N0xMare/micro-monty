import json
from datetime import date
from collections import defaultdict
rows = json.loads(await query_database({"sql": "SELECT order_date, total FROM orders WHERE status='completed' AND order_date BETWEEN '2026-01-01' AND '2026-03-31'"}))
weeks = defaultdict(float)
for r in rows:
    year, week, _ = date.fromisoformat(r["order_date"]).isocalendar()
    weeks[f"{year}-W{week:02d}"] += r["total"]
await submit_answer({"weeks": {w: round(v, 2) for w, v in sorted(weeks.items())}})
