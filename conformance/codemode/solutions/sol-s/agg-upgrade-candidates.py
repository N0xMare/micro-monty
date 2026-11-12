rows = await query_database({"sql": "SELECT c.id, c.name, ROUND(SUM(o.total), 2) AS spend FROM customers c JOIN orders o ON o.customer_id = c.id WHERE c.tier = 'bronze' AND o.status = 'completed' GROUP BY c.id, c.name HAVING SUM(o.total) > 2000 ORDER BY SUM(o.total) DESC, c.id"})
await submit_answer({"candidates": rows})
