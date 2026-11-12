import asyncio, json

rows_s, rate_s = await asyncio.gather(
    query_database({"sql": """
SELECT o.currency, SUM(o.total) AS revenue, COUNT(*) AS n
FROM orders o JOIN customers c ON c.id = o.customer_id
WHERE c.country = 'GB' AND o.status = 'completed'
GROUP BY o.currency
"""}),
    get_exchange_rate({"base": "USD", "quote": "GBP"}),
)
rows = json.loads(rows_s)
rate = json.loads(rate_s)["rate"]
print(rows, rate)
assert all(r["currency"] == "USD" for r in rows), rows
usd = sum(r["revenue"] or 0 for r in rows)
total_gbp = round(usd * rate, 2)
print(usd, total_gbp)
print(await submit_answer({"total_gbp": total_gbp}))
