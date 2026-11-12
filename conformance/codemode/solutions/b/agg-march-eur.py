import asyncio, json

rows_raw, rate_raw = await asyncio.gather(
    query_database({"sql": """
SELECT currency, COALESCE(SUM(total), 0) AS total, COUNT(*) AS n
FROM orders
WHERE status = 'completed' AND order_date BETWEEN '2026-03-01' AND '2026-03-31'
GROUP BY currency
"""}),
    get_exchange_rate({"base": "USD", "quote": "EUR"}),
)
rows = json.loads(rows_raw)
rate = json.loads(rate_raw)["rate"]
print(rows, rate)

total_usd = sum(r["total"] for r in rows)
total_eur = total_usd * rate
answer = {"total_usd": round(total_usd, 2), "total_eur": round(total_eur, 2)}
print(answer)
print(await submit_answer(answer))
