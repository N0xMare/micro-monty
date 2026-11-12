import json

rows = json.loads(await query_database({"sql": "SELECT p.category, SUM(i.qty) AS units FROM orders o JOIN order_items i ON i.order_id = o.id JOIN products p ON p.id = i.product_id WHERE o.status = 'completed' AND o.order_date >= '2026-01-01' AND o.order_date < '2026-04-01' GROUP BY p.category ORDER BY units DESC, p.category LIMIT 1"}))
await submit_answer(rows[0])
