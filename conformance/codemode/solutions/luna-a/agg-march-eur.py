import json
r = json.loads(await query_database({"sql": "SELECT COALESCE(SUM(total),0) AS usd FROM orders WHERE status='completed' AND order_date >= '2026-03-01' AND order_date < '2026-04-01'"}))
rate = json.loads(await get_exchange_rate({"base": "USD", "quote": "EUR"}))
usd = r[0]["usd"]
await submit_answer({"total_usd": round(usd, 2), "total_eur": round(usd * rate["rate"], 2)})
