import json
from datetime import date
from collections import Counter
rows = json.loads(await query_database({"sql": "SELECT order_date FROM orders WHERE order_date BETWEEN '2026-01-01' AND '2026-12-31'"}))
names = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
await submit_answer({"counts": dict(Counter(names[date.fromisoformat(r["order_date"]).weekday()] for r in rows))})
