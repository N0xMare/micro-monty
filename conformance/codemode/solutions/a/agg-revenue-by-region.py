import json

rows = json.loads(await query_database({"sql": """
SELECT c.region, o.currency, SUM(o.total) AS revenue, COUNT(*) AS n
FROM orders o JOIN customers c ON c.id = o.customer_id
WHERE o.status = 'completed'
  AND o.order_date >= '2026-02-01' AND o.order_date < '2026-03-01'
GROUP BY c.region, o.currency
ORDER BY c.region
"""}))
print(rows)
assert all(r["currency"] == "USD" for r in rows), rows
totals = {}
for r in rows:
    totals[r["region"]] = totals.get(r["region"], 0.0) + (r["revenue"] or 0)
totals = {k: round(v, 2) for k, v in totals.items() if v}
print(totals)
print(await submit_answer({"totals": totals}))
