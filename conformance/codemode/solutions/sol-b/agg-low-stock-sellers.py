import json
rows = json.loads(await query_database({"sql": "SELECT p.id FROM products p JOIN order_items i ON i.product_id=p.id JOIN orders o ON o.id=i.order_id WHERE p.stock<10 AND o.status='completed' GROUP BY p.id HAVING SUM(i.qty)>=5 ORDER BY p.id"}))
await submit_answer({"product_ids": [r["id"] for r in rows]})
