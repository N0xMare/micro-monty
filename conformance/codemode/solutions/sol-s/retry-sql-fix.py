try:
    rows = await query_database({"sql": "SELECT region, COUNT(*) AS n FROM customer GROUP BY region"})
except Exception as error:
    message = str(error).lower()
    if "customer" not in message or not any(s in message for s in ["no such table", "does not exist", "unknown table"]):
        raise
    rows = await query_database({"sql": "SELECT region, COUNT(*) AS n FROM customers GROUP BY region"})
await submit_answer({"counts": {r["region"]: r["n"] for r in rows}})
