import json

rows = json.loads(await query_database({"sql": "SELECT total FROM orders WHERE status = 'completed'"}))
labels = [f"{lo}-{lo + 499}" for lo in range(0, 3000, 500)] + ["3000+"]
buckets = {label: 0 for label in labels}
for row in rows:
    total = row["total"]
    label = "3000+" if total >= 3000 else f"{int(total // 500) * 500}-{int(total // 500) * 500 + 499}"
    buckets[label] += 1
await submit_answer({"buckets": buckets})
