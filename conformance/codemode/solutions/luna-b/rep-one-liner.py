import json

rows = json.loads(await query_database({"sql": "SELECT status, COUNT(*) AS n FROM orders GROUP BY status"}))
counts = {row["status"]: row["n"] for row in rows}
statuses = ["completed", "pending", "cancelled", "refunded"]
total = sum(counts.values())
parts = [f"{status} {round(counts.get(status, 0) / total * 100) if total else 0}%" for status in statuses]
await post_message({"channel": "#sales", "text": f"Orders: {total} ({', '.join(parts)})"})
