import asyncio, json

sql = "SELECT COALESCE(SUM(total),0) AS total FROM orders WHERE status='completed' AND order_date >= '2026-03-01' AND order_date < '2026-04-01'"
total_raw, rate_raw = await asyncio.gather(query_database({"sql": sql}), get_exchange_rate({"base": "USD", "quote": "EUR"}))
total = json.loads(total_raw)[0]["total"]
rate = json.loads(rate_raw)["rate"]
await submit_answer({"total_usd": round(total, 2), "total_eur": round(total * rate, 2)})
