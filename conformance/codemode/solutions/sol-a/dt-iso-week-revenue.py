import json
from collections import defaultdict
from datetime import date
from decimal import Decimal

rows = json.loads(await query_database({"sql": "SELECT order_date, total FROM orders WHERE status = 'completed' AND order_date >= '2026-01-01' AND order_date < '2026-04-01'"}))
weeks = defaultdict(Decimal)
for row in rows:
    year, week, _ = date.fromisoformat(row["order_date"]).isocalendar()
    weeks[f"{year}-W{week:02d}"] += Decimal(str(row["total"]))
await submit_answer({"weeks": {w: float(round(weeks[w], 2)) for w in sorted(weeks)}})
