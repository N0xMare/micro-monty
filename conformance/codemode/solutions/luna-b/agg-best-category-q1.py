import json

sql = "SELECT p.category, SUM(oi.qty) AS units FROM orders o JOIN order_items oi ON oi.order_id=o.id JOIN products p ON p.id=oi.product_id WHERE o.status='completed' AND o.order_date >= '2026-01-01' AND o.order_date < '2026-04-01' GROUP BY p.category ORDER BY units DESC, p.category ASC LIMIT 1"
row = json.loads(await query_database({"sql": sql}))[0]
await submit_answer({"category": row["category"], "units": row["units"]})
