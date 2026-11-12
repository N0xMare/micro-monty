import asyncio, json

sql = "SELECT COALESCE(SUM(o.total),0) AS revenue FROM orders o JOIN customers c ON c.id=o.customer_id WHERE o.status='completed' AND c.country='GB'"
rev_raw, rate_raw = await asyncio.gather(query_database({"sql": sql}), get_exchange_rate({"base": "USD", "quote": "GBP"}))
revenue = json.loads(rev_raw)[0]["revenue"]
rate = json.loads(rate_raw)["rate"]
await submit_answer({"total_gbp": round(revenue * rate, 2)})
