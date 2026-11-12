import json
from collections import Counter
from datetime import date
rows = json.loads(await query_database({"sql": "SELECT order_date FROM orders WHERE order_date >= '2026-01-01' AND order_date < '2027-01-01'"}))
counts = Counter(date.fromisoformat(x["order_date"]).strftime("%A") for x in rows)
await submit_answer({"counts": dict(counts)})
