import datetime
from collections import Counter

rows = await query_database({"sql": "SELECT order_date FROM orders WHERE order_date >= '2026-01-01' AND order_date < '2027-01-01'"})
counts = Counter(datetime.date.fromisoformat(row["order_date"]).strftime("%A") for row in rows)
await submit_answer({"counts": dict(counts)})
