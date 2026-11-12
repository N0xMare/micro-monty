rows = await query_database({"sql": "SELECT c.region, ROUND(100.0 * SUM(CASE WHEN o.status = 'cancelled' THEN 1 ELSE 0 END) / COUNT(*), 1) AS rate FROM orders o JOIN customers c ON c.id = o.customer_id GROUP BY c.region"})
await submit_answer({"rates": {r["region"]: r["rate"] for r in rows}})
