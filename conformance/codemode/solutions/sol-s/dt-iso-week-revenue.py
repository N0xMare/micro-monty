from datetime import date
from decimal import Decimal
from collections import defaultdict

rows = await query_database({"sql": "SELECT order_date, total FROM orders WHERE status = 'completed' AND order_date >= '2026-01-01' AND order_date < '2026-04-01'"})
totals = defaultdict(Decimal)
for row in rows:
    year, week, _ = date.fromisoformat(row["order_date"]).isocalendar()
    totals[f"{year}-W{week:02d}"] += Decimal(str(row["total"]))
await submit_answer({"weeks": {week: float(round(total, 2)) for week, total in sorted(totals.items())}})
