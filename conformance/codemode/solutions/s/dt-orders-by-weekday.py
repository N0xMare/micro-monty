from datetime import date
from collections import Counter

rows = await query_database({"sql": "SELECT order_date FROM orders WHERE order_date >= '2026-01-01' AND order_date < '2027-01-01'"})
names = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
c = Counter(names[date.fromisoformat(r["order_date"][:10]).weekday()] for r in rows)
counts = {n: c[n] for n in names if c[n]}
print(counts)
await submit_answer({"counts": counts})
