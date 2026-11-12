import json

sql = "SELECT region, COUNT(*) AS n FROM customer GROUP BY region"
try:
    rows = json.loads(await query_database({"sql": sql}))
except Exception as e:
    print(f"query failed: {type(e).__name__}: {e}")
    # The schema names the table "customers", not "customer".
    sql = "SELECT region, COUNT(*) AS n FROM customers GROUP BY region"
    rows = json.loads(await query_database({"sql": sql}))

counts = {r["region"]: r["n"] for r in rows}
print(counts)
print(await submit_answer({"counts": counts}))
