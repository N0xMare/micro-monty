rows = await query_database({"sql": "SELECT AVG(units) AS average_units FROM (SELECT o.id, COALESCE(SUM(oi.qty), 0) AS units FROM orders o LEFT JOIN order_items oi ON oi.order_id = o.id WHERE o.status = 'completed' GROUP BY o.id)"})
await submit_answer({"average_units": round(float(rows[0]["average_units"] or 0), 2)})
