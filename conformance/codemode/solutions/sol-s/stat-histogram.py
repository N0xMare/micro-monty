rows = await query_database({"sql": "SELECT total FROM orders WHERE status = 'completed'"})
buckets = {f"{lo}-{lo + 499}": 0 for lo in range(0, 3000, 500)}
buckets["3000+"] = 0
for row in rows:
    total = row["total"]
    if total >= 3000:
        buckets["3000+"] += 1
    elif total >= 0:
        lo = int(total // 500) * 500
        buckets[f"{lo}-{lo + 499}"] += 1
await submit_answer({"buckets": buckets})
