from datetime import date
from collections import defaultdict

rows = await query_database({"sql": """
SELECT order_date, SUM(total) AS total
FROM orders
WHERE status = 'completed' AND order_date >= '2026-01-01' AND order_date <= '2026-03-31'
GROUP BY order_date
"""})
weeks = defaultdict(float)
for r in rows:
    y, w, _ = date.fromisoformat(r["order_date"]).isocalendar()
    weeks[f"{y}-W{w:02d}"] += r["total"] or 0
weeks = {k: round(v, 2) for k, v in sorted(weeks.items())}
print(weeks)
print(await submit_answer({"weeks": weeks}))
