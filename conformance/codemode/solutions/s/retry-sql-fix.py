sql = "SELECT region, COUNT(*) AS n FROM customer GROUP BY region"
try:
    rows = await query_database({"sql": sql})
except Exception as e:
    print("original query failed:", e)
    # the table is named "customers", not "customer"
    sql = "SELECT region, COUNT(*) AS n FROM customers GROUP BY region"
    rows = await query_database({"sql": sql})

counts = {r["region"]: r["n"] for r in rows}
print(sql)
print(counts)
print(await submit_answer({"counts": counts}))
