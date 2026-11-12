rows = await query_database({"sql": "SELECT status, COUNT(*) AS n FROM orders GROUP BY status"})
counts = {r["status"]: r["n"] for r in rows}
statuses = ["completed", "pending", "cancelled", "refunded"]
total = sum(counts.values())
parts = [f"{s} {round(counts.get(s, 0) / total * 100) if total else 0}%" for s in statuses]
await post_message({"channel": "#sales", "text": f"Orders: {total} ({', '.join(parts)})"})
