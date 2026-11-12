rows = await query_database({"sql": """
SELECT customer_id, COUNT(*) AS n
FROM orders
WHERE status = 'completed'
GROUP BY customer_id
HAVING COUNT(*) >= 3
ORDER BY customer_id
"""})
print(rows)
ids = sorted(r["customer_id"] for r in rows)
print(await submit_answer({"count": len(ids), "customer_ids": ids}))
