import json

sql = "SELECT c.name, ROUND(SUM(o.total),2) AS total FROM orders o JOIN customers c ON c.id=o.customer_id WHERE o.status='completed' GROUP BY c.id, c.name ORDER BY SUM(o.total) DESC, c.id ASC LIMIT 5"
rows = json.loads(await query_database({"sql": sql}))
await submit_answer({"top": [{"name": r["name"], "total": r["total"]} for r in rows]})
