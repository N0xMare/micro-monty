import json

sql = "SELECT p.id FROM products p JOIN order_items oi ON oi.product_id=p.id JOIN orders o ON o.id=oi.order_id AND o.status='completed' WHERE p.stock<10 GROUP BY p.id HAVING SUM(oi.qty)>=5 ORDER BY p.id"
rows = json.loads(await query_database({"sql": sql}))
await submit_answer({"product_ids": [r["id"] for r in rows]})
