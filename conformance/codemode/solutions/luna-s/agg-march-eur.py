rows, rate = await asyncio.gather(query_database({"sql": "SELECT COALESCE(SUM(total), 0) AS total_usd FROM orders WHERE status = 'completed' AND order_date >= '2026-03-01' AND order_date < '2026-04-01'"}), get_exchange_rate({"base": "USD", "quote": "EUR"}))
total_usd = round(float(rows[0]["total_usd"]), 2)
await submit_answer({"total_usd": total_usd, "total_eur": round(total_usd * float(rate["rate"]), 2)})
