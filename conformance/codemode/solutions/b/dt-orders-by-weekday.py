import json
from collections import Counter
from datetime import date

rows = json.loads(await query_database({"sql": "SELECT id, order_date FROM orders WHERE order_date >= '2026-01-01' AND order_date < '2027-01-01'"}))

counts = Counter(date.fromisoformat(r["order_date"][:10]).strftime("%A") for r in rows)
order = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
result = {d: counts[d] for d in order if counts[d]}
print(len(rows), result)
print(await submit_answer({"counts": result}))
