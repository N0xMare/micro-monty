import json

rows = json.loads(await query_database({"sql": """
SELECT c.id, c.name, SUM(o.total) AS spend
FROM customers c JOIN orders o ON o.customer_id = c.id
WHERE c.tier = 'bronze' AND o.status = 'completed'
GROUP BY c.id, c.name
HAVING SUM(o.total) > 2000
ORDER BY spend DESC
"""}))
print(rows)
candidates = [{"id": r["id"], "name": r["name"], "spend": round(r["spend"], 2)} for r in rows]
print(await submit_answer({"candidates": candidates}))
