import json

sql = "SELECT c.region, ROUND(100.0 * SUM(CASE WHEN o.status='cancelled' THEN 1 ELSE 0 END) / COUNT(o.id), 1) AS rate FROM customers c JOIN orders o ON o.customer_id=c.id GROUP BY c.region"
rows = json.loads(await query_database({"sql": sql}))
await submit_answer({"rates": {r["region"]: r["rate"] for r in rows}})
