import json

try:
    raw = await query_database({"sql": "SELECT region, COUNT(*) AS n FROM customer GROUP BY region"})
except Exception as error:
    message = str(error).lower()
    if "customer" not in message or not any(term in message for term in ["no such table", "does not exist", "unknown table", "not found"]):
        raise
    raw = await query_database({"sql": "SELECT region, COUNT(*) AS n FROM customers GROUP BY region"})
rows = json.loads(raw)
await submit_answer({"counts": {row["region"]: row["n"] for row in rows}})
