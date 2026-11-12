import json

sql = "SELECT c.id, c.name, ROUND(SUM(o.total),2) AS spend FROM customers c JOIN orders o ON o.customer_id=c.id AND o.status='completed' WHERE c.tier='bronze' GROUP BY c.id, c.name HAVING SUM(o.total)>2000 ORDER BY spend DESC, c.id ASC"
rows = json.loads(await query_database({"sql": sql}))
await submit_answer({"candidates": [{"id": r["id"], "name": r["name"], "spend": r["spend"]} for r in rows]})
