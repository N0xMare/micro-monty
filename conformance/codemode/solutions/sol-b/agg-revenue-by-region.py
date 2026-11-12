import json
rows = json.loads(await query_database({"sql": "SELECT c.region, SUM(o.total) AS total FROM orders o JOIN customers c ON c.id=o.customer_id WHERE o.status='completed' AND o.order_date BETWEEN '2026-02-01' AND '2026-02-28' GROUP BY c.region"}))
await submit_answer({"totals": {r["region"]: round(r["total"], 2) for r in rows}})
