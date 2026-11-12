import json

sql = "SELECT c.id FROM customers c LEFT JOIN orders o ON o.customer_id=c.id WHERE c.signup_date >= '2025-01-01' AND c.signup_date < '2026-01-01' AND o.id IS NULL ORDER BY c.id"
rows = json.loads(await query_database({"sql": sql}))
await submit_answer({"customer_ids": [r["id"] for r in rows]})
