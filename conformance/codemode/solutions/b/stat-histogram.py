import json
import math

rows = json.loads(await query_database({"sql": "SELECT total FROM orders WHERE status = 'completed'"}))
labels = [f"{lo}-{lo + 499}" for lo in range(0, 3000, 500)] + ["3000+"]
buckets = {label: 0 for label in labels}
for row in rows:
    t = row["total"]
    if t is None:
        continue
    idx = min(max(math.floor(t / 500), 0), 6)
    buckets[labels[idx]] += 1

print(len(rows), "completed orders", buckets)
print(await submit_answer({"buckets": buckets}))
