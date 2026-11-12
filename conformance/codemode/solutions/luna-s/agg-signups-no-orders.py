rows = await query_database({"sql": "SELECT c.id FROM customers c WHERE c.signup_date >= '2025-01-01' AND c.signup_date < '2026-01-01' AND NOT EXISTS (SELECT 1 FROM orders o WHERE o.customer_id = c.id) ORDER BY c.id"})
await submit_answer({"customer_ids": [int(row["id"]) for row in rows]})
