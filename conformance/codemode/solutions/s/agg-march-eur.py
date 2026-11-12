import asyncio

rows, rate = await asyncio.gather(
    query_database({"sql": """
SELECT currency, SUM(total) AS total, COUNT(*) AS n
FROM orders
WHERE status = 'completed' AND order_date >= '2026-03-01' AND order_date <= '2026-03-31'
GROUP BY currency
"""}),
    get_exchange_rate({"base": "USD", "quote": "EUR"}),
)
print(rows, rate)
total_usd = sum(r["total"] or 0 for r in rows)
total_eur = total_usd * rate["rate"]
answer = {"total_usd": round(total_usd, 2), "total_eur": round(total_eur, 2)}
print(answer)
print(await submit_answer(answer))
