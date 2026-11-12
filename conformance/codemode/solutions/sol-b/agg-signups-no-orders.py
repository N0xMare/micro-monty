import json
rows = json.loads(await query_database({"sql": "SELECT c.id FROM customers c WHERE c.signup_date BETWEEN '2025-01-01' AND '2025-12-31' AND NOT EXISTS (SELECT 1 FROM orders o WHERE o.customer_id=c.id) ORDER BY c.id"}))
await submit_answer({"customer_ids": [r["id"] for r in rows]})
