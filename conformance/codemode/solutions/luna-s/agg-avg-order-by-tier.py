rows = await query_database({"sql": "SELECT c.tier, AVG(o.total) AS average FROM customers c JOIN orders o ON o.customer_id = c.id WHERE o.status = 'completed' GROUP BY c.tier"})
averages = {row["tier"]: round(float(row["average"]), 2) for row in rows}
await submit_answer({"averages": averages})
