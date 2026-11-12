rows = await query_database({"sql": "SELECT c.region, SUM(o.total) AS total FROM orders o JOIN customers c ON c.id = o.customer_id WHERE o.status = 'completed' AND o.order_date >= '2026-02-01' AND o.order_date < '2026-03-01' GROUP BY c.region"})
await submit_answer({"totals": {row["region"]: round(float(row["total"]), 2) for row in rows}})
