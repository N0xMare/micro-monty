import json
from datetime import date
from collections import Counter

rows = json.loads(await query_database({"sql": "SELECT order_date FROM orders WHERE order_date >= '2026-01-01' AND order_date < '2027-01-01'"}))
counts = Counter(date.fromisoformat(r["order_date"][:10]).strftime("%A") for r in rows)
counts = dict(counts)
print(len(rows), counts)
print(await submit_answer({"counts": counts}))
