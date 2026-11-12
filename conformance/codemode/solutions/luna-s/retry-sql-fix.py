try:
    rows = await query_database({"sql": "SELECT region, COUNT(*) AS n FROM customer GROUP BY region"})
except Exception:
    rows = await query_database({"sql": "SELECT region, COUNT(*) AS n FROM customers GROUP BY region"})
await submit_answer({"counts": {row["region"]: row["n"] for row in rows}})
