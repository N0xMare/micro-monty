rows = await query_database({"sql": """
SELECT c.id AS id, c.name AS name, SUM(o.total) AS total
FROM orders o
JOIN customers c ON c.id = o.customer_id
WHERE o.status = 'completed'
GROUP BY c.id, c.name
ORDER BY total DESC
LIMIT 6
"""})
print(rows)  # 6th row shown to spot ties at the boundary
top = [{"name": r["name"], "total": round(r["total"], 2)} for r in rows[:5]]
print(await submit_answer({"top": top}))
