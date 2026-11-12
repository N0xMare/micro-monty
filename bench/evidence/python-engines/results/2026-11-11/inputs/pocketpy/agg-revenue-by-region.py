rows = query_database({'sql': "\nSELECT c.region AS region, SUM(o.total) AS total\nFROM orders o\nJOIN customers c ON c.id = o.customer_id\nWHERE o.status = 'completed'\n  AND o.order_date >= '2026-02-01' AND o.order_date < '2026-03-01'\nGROUP BY c.region\nORDER BY c.region\n"})
print(rows)
totals = {r['region']: round(r['total'], 2) for r in rows if r['total']}
print(submit_answer({'totals': totals}))
