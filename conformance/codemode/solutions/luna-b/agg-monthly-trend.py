import json

sql = "SELECT substr(order_date,1,7) AS month, ROUND(SUM(total),2) AS total FROM orders WHERE status='completed' AND order_date >= '2025-10-01' AND order_date < '2026-04-01' GROUP BY month"
rows = {r["month"]: r["total"] for r in json.loads(await query_database({"sql": sql}))}
months = [{"month": f"{year}-{month:02d}", "total": rows.get(f"{year}-{month:02d}", 0)} for year, month in [(2025, 10), (2025, 11), (2025, 12), (2026, 1), (2026, 2), (2026, 3)]]
await submit_answer({"months": months})
