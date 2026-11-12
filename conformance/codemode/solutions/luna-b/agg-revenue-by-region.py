import json

sql = "SELECT c.region, ROUND(SUM(o.total),2) AS total FROM orders o JOIN customers c ON c.id=o.customer_id WHERE o.status='completed' AND o.order_date >= '2026-02-01' AND o.order_date < '2026-03-01' GROUP BY c.region"
rows = json.loads(await query_database({"sql": sql}))
await submit_answer({"totals": {r["region"]: r["total"] for r in rows}})
