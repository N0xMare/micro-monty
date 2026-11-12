import asyncio, json

rows_s, rate_s = await asyncio.gather(
    query_database({"sql": """
SELECT currency, SUM(total) AS total, COUNT(*) AS n
FROM orders
WHERE status = 'completed' AND order_date BETWEEN '2026-03-01' AND '2026-03-31'
GROUP BY currency
"""}),
    get_exchange_rate({"base": "USD", "quote": "EUR"}),
)
rows = json.loads(rows_s)
rate = json.loads(rate_s)["rate"]
print(rows, rate)
assert all(r["currency"] == "USD" for r in rows), rows
usd = sum(r["total"] or 0 for r in rows)
total_usd = round(usd, 2)
total_eur = round(usd * rate, 2)
print(total_usd, total_eur)
print(await submit_answer({"total_usd": total_usd, "total_eur": total_eur}))
