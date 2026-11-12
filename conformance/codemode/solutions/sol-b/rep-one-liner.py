import json

rows = json.loads(await query_database({"sql": "SELECT status, COUNT(*) AS n FROM orders GROUP BY status"}))
counts = {r["status"]: r["n"] for r in rows}
total = sum(counts.values())
parts = [f"{s} {counts.get(s, 0) / total * 100 if total else 0:.0f}%" for s in ["completed", "pending", "cancelled", "refunded"]]
await post_message({"channel": "#sales", "text": f"Orders: {total} ({', '.join(parts)})"})
