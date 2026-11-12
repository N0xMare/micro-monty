import json

try:
    raw = await query_database({"sql": "SELECT region, COUNT(*) AS n FROM customer GROUP BY region"})
except Exception as exc:
    message = str(exc)
    print(message)
    if "customer" not in message.lower() or not any(term in message.lower() for term in ["no such table", "does not exist", "unknown table", "not found"]):
        raise
    raw = await query_database({"sql": "SELECT region, COUNT(*) AS n FROM customers GROUP BY region"})
rows = json.loads(raw)
await submit_answer({"counts": {r["region"]: r["n"] for r in rows}})
