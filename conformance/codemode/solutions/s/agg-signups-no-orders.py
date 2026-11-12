rows = await query_database({"sql": """
SELECT c.id AS id
FROM customers c
WHERE c.signup_date >= '2025-01-01' AND c.signup_date <= '2025-12-31'
  AND NOT EXISTS (SELECT 1 FROM orders o WHERE o.customer_id = c.id)
ORDER BY c.id
"""})
print(rows)
ids = sorted(r["id"] for r in rows)
print(await submit_answer({"customer_ids": ids}))
