import json

rows = json.loads(await query_database({"sql": """
SELECT p.category, SUM(oi.qty) AS units
FROM orders o
JOIN order_items oi ON oi.order_id = o.id
JOIN products p ON p.id = oi.product_id
WHERE o.status = 'completed'
  AND o.order_date BETWEEN '2026-01-01' AND '2026-03-31'
GROUP BY p.category
ORDER BY units DESC
"""}))
print(rows)
top = rows[0]
print(await submit_answer({"category": top["category"], "units": int(top["units"])}))
