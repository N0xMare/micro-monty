rows = await query_database({"sql": "SELECT customer_id, COUNT(*) AS completed_orders FROM orders WHERE status = 'completed' GROUP BY customer_id HAVING COUNT(*) >= 3 ORDER BY customer_id"})
customer_ids = [int(row["customer_id"]) for row in rows]
await submit_answer({"count": len(customer_ids), "customer_ids": customer_ids})
