import json
r = json.loads(await query_database({"sql": "SELECT customer_id FROM orders WHERE status='completed' GROUP BY customer_id HAVING COUNT(*)>=3 ORDER BY customer_id"}))
ids = [x["customer_id"] for x in r]
await submit_answer({"count": len(ids), "customer_ids": ids})
