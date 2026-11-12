import json

sql = "SELECT region, COUNT(*) AS n FROM customer GROUP BY region"
try:
    rows = json.loads(await query_database({"sql": sql}))
except Exception as e:
    print("query failed:", e)
    # The schema's table is named "customers", not "customer"
    sql = "SELECT region, COUNT(*) AS n FROM customers GROUP BY region"
    rows = json.loads(await query_database({"sql": sql}))

counts = {r["region"]: r["n"] for r in rows}
print(sql)
print(counts)
print(await submit_answer({"counts": counts}))
