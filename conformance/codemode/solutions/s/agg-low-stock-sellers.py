rows = await query_database({"sql": """
SELECT p.id AS id, p.stock AS stock, SUM(oi.qty) AS sold
FROM products p
JOIN order_items oi ON oi.product_id = p.id
JOIN orders o ON o.id = oi.order_id
WHERE o.status = 'completed' AND p.stock < 10
GROUP BY p.id, p.stock
HAVING SUM(oi.qty) >= 5
ORDER BY p.id
"""})
print(rows)
product_ids = sorted(r["id"] for r in rows)
print(await submit_answer({"product_ids": product_ids}))
