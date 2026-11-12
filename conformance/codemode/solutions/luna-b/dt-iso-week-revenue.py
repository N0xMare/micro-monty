import json
from datetime import date

sql = "SELECT order_date, total FROM orders WHERE status='completed' AND order_date >= '2026-01-01' AND order_date < '2026-04-01'"
rows = json.loads(await query_database({"sql": sql}))
weeks = {}
for r in rows:
    d = date.fromisoformat(r["order_date"])
    iso = d.isocalendar()
    label = f"{iso.year}-W{iso.week:02d}"
    weeks[label] = weeks.get(label, 0) + r["total"]
await submit_answer({"weeks": {k: round(v, 2) for k, v in weeks.items()}})
