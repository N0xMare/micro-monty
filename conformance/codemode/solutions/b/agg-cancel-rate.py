import json

rows = json.loads(await query_database({"sql": """
SELECT c.region,
       SUM(CASE WHEN o.status = 'cancelled' THEN 1 ELSE 0 END) AS cancelled,
       COUNT(*) AS total
FROM orders o JOIN customers c ON c.id = o.customer_id
GROUP BY c.region
ORDER BY c.region
"""}))
print(rows)
rates = {r["region"]: round(r["cancelled"] * 100.0 / r["total"], 1) for r in rows if r["total"]}
print(rates)
print(await submit_answer({"rates": rates}))
