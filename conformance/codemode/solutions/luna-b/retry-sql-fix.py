import json

try:
    raw = await query_database({"sql": "SELECT region, COUNT(*) AS n FROM customer GROUP BY region"})
except Exception:
    raw = await query_database({"sql": "SELECT region, COUNT(*) AS n FROM customers GROUP BY region"})
rows = json.loads(raw)
await submit_answer({"counts": {row["region"]: row["n"] for row in rows}})
