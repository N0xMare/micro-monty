import json

rows = json.loads(await query_database({"sql": "SELECT c.name, ROUND(SUM(o.total), 2) AS total FROM customers c JOIN orders o ON o.customer_id = c.id WHERE o.status = 'completed' GROUP BY c.id, c.name ORDER BY SUM(o.total) DESC, c.id LIMIT 5"}))
await submit_answer({"top": rows})
