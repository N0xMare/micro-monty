import asyncio

rows = await query_database({"sql": """
SELECT o.currency AS currency, SUM(o.total) AS total, COUNT(*) AS n
FROM orders o
JOIN customers c ON c.id = o.customer_id
WHERE o.status = 'completed' AND UPPER(TRIM(c.country)) = 'GB'
GROUP BY o.currency
"""})
print(rows)

currencies = {(r["currency"] or "USD").upper() for r in rows}
rates = {}
need = [c for c in currencies if c != "GBP"]
results = await asyncio.gather(*(get_exchange_rate({"base": c, "quote": "GBP"}) for c in need))
for c, r in zip(need, results):
    rates[c] = r["rate"]
rates["GBP"] = 1.0
print(rates)

total_gbp = sum((r["total"] or 0) * rates[(r["currency"] or "USD").upper()] for r in rows)
total_gbp = round(total_gbp, 2)
print(total_gbp)
print(await submit_answer({"total_gbp": total_gbp}))
