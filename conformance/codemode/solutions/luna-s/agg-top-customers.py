rows = await query_database({"sql": "SELECT c.name, SUM(o.total) AS total FROM customers c JOIN orders o ON o.customer_id = c.id WHERE o.status = 'completed' GROUP BY c.id, c.name ORDER BY total DESC, c.name ASC LIMIT 5"})
await submit_answer({"top": [{"name": row["name"], "total": round(float(row["total"]), 2)} for row in rows]})
