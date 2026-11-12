import asyncio, json
raw, rate = await asyncio.gather(query_database({"sql": "SELECT COALESCE(SUM(total),0) AS total FROM orders WHERE status='completed' AND order_date BETWEEN '2026-03-01' AND '2026-03-31'"}), get_exchange_rate({"base": "USD", "quote": "EUR"}))
total = json.loads(raw)[0]["total"]
await submit_answer({"total_usd": round(total, 2), "total_eur": round(total * json.loads(rate)["rate"], 2)})
