import json

sql = "SELECT customer_id FROM orders WHERE status='completed' GROUP BY customer_id HAVING COUNT(*)>=3 ORDER BY customer_id"
rows = json.loads(await query_database({"sql": sql}))
await submit_answer({"count": len(rows), "customer_ids": [r["customer_id"] for r in rows]})
