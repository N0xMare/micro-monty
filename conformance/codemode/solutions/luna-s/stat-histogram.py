rows = await query_database({"sql": "SELECT total FROM orders WHERE status = 'completed'"})
labels = [f"{lo}-{lo + 499}" for lo in range(0, 3000, 500)] + ["3000+"]
buckets = {label: 0 for label in labels}
for row in rows:
    value = row["total"]
    if value >= 3000:
        buckets["3000+"] += 1
    elif value >= 0:
        buckets[f"{int(value // 500) * 500}-{int(value // 500) * 500 + 499}"] += 1
await submit_answer({"buckets": buckets})
