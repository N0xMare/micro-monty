import json
r = json.loads(await query_database({"sql": "SELECT substr(order_date,1,7) AS month, ROUND(SUM(total),2) AS total FROM orders WHERE status='completed' AND order_date >= '2025-10-01' AND order_date < '2026-04-01' GROUP BY substr(order_date,1,7)"}))
totals = {x["month"]: x["total"] for x in r}
months = [f"{y:04d}-{m:02d}" for y, m in ((2025,10),(2025,11),(2025,12),(2026,1),(2026,2),(2026,3))]
await submit_answer({"months": [{"month": m, "total": totals.get(m, 0)} for m in months]})
