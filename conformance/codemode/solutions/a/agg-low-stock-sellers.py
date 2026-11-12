import json

rows = json.loads(await query_database({"sql": """
SELECT p.id, p.name, p.stock, SUM(oi.qty) AS sold
FROM products p
JOIN order_items oi ON oi.product_id = p.id
JOIN orders o ON o.id = oi.order_id
WHERE o.status = 'completed' AND p.stock < 10
GROUP BY p.id, p.name, p.stock
HAVING SUM(oi.qty) >= 5
ORDER BY p.id
"""}))
print(rows)
ids = sorted(r["id"] for r in rows)
print(await submit_answer({"product_ids": ids}))
