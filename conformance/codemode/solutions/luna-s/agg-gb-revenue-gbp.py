rows, rate = await query_database({"sql": "SELECT COALESCE(SUM(o.total), 0) AS total_usd FROM orders o JOIN customers c ON c.id = o.customer_id WHERE o.status = 'completed' AND c.country = 'GB'"}), await get_exchange_rate({"base": "USD", "quote": "GBP"})
await submit_answer({"total_gbp": round(float(rows[0]["total_usd"]) * float(rate["rate"]), 2)})
