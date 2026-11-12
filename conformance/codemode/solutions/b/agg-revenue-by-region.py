import json

rows = json.loads(await query_database({"sql": """
SELECT c.region, SUM(o.total) AS revenue, COUNT(*) AS n
FROM orders o JOIN customers c ON c.id = o.customer_id
WHERE o.status = 'completed'
  AND o.order_date BETWEEN '2026-02-01' AND '2026-02-29'
GROUP BY c.region
ORDER BY c.region
"""}))
print(rows)
totals = {r["region"]: round(r["revenue"], 2) for r in rows if r["revenue"]}
print(await submit_answer({"totals": totals}))
