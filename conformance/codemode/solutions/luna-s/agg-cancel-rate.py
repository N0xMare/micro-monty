rows = await query_database({"sql": "SELECT c.region, 100.0 * SUM(CASE WHEN o.status = 'cancelled' THEN 1 ELSE 0 END) / COUNT(*) AS rate FROM orders o JOIN customers c ON c.id = o.customer_id GROUP BY c.region"})
rates = {row["region"]: round(float(row["rate"]), 1) for row in rows}
await submit_answer({"rates": rates})
