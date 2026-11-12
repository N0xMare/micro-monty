import asyncio, json
raw, rate = await asyncio.gather(query_database({"sql": "SELECT COALESCE(SUM(o.total),0) AS total FROM orders o JOIN customers c ON c.id=o.customer_id WHERE o.status='completed' AND c.country='GB'"}), get_exchange_rate({"base": "USD", "quote": "GBP"}))
await submit_answer({"total_gbp": round(json.loads(raw)[0]["total"] * json.loads(rate)["rate"], 2)})
