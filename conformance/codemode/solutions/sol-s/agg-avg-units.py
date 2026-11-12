rows = await query_database({"sql": "SELECT ROUND(AVG(units), 2) AS average_units FROM (SELECT o.id, COALESCE(SUM(i.qty), 0) AS units FROM orders o LEFT JOIN order_items i ON i.order_id = o.id WHERE o.status = 'completed' GROUP BY o.id)"})
await submit_answer({"average_units": rows[0]["average_units"]})
