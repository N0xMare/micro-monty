import json

rows = json.loads(await query_database({"sql": """
SELECT c.id, c.name, SUM(o.total) AS total
FROM orders o JOIN customers c ON c.id = o.customer_id
WHERE o.status = 'completed'
GROUP BY c.id, c.name
ORDER BY total DESC
LIMIT 7
"""}))
print(rows)  # extra rows shown to spot ties at the cutoff
top = [{"name": r["name"], "total": round(r["total"], 2)} for r in rows[:5]]
print(await submit_answer({"top": top}))
