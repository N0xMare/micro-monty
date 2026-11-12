import json
r = json.loads(await query_database({"sql": "SELECT COALESCE(SUM(o.total),0) AS usd FROM orders o JOIN customers c ON c.id=o.customer_id WHERE o.status='completed' AND c.country='GB'"}))
rate = json.loads(await get_exchange_rate({"base": "USD", "quote": "GBP"}))
await submit_answer({"total_gbp": round(r[0]["usd"] * rate["rate"], 2)})
