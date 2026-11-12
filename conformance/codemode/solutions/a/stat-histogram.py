import json

rows = json.loads(await query_database({"sql": "SELECT total FROM orders WHERE status = 'completed'"}))

labels = [f"{lo}-{lo + 499}" for lo in range(0, 3000, 500)] + ["3000+"]
buckets = {label: 0 for label in labels}
for row in rows:
    t = row["total"]
    if t is None:
        continue
    if t >= 3000:
        buckets["3000+"] += 1
    else:
        lo = int(t // 500) * 500
        buckets[f"{lo}-{lo + 499}"] += 1

print(len(rows), buckets)
print(await submit_answer({"buckets": buckets}))
