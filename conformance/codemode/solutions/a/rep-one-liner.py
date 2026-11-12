import json

rows = json.loads(await query_database({"sql": "SELECT status, COUNT(*) AS n FROM orders GROUP BY status"}))
counts = {r["status"]: r["n"] for r in rows}
total = sum(counts.values())
parts = ", ".join(
    f"{s} {round(counts.get(s, 0) / total * 100) if total else 0}%"
    for s in ["completed", "pending", "cancelled", "refunded"])
text = f"Orders: {total} ({parts})"
print(text)
print(await post_message({"channel": "#sales", "text": text}))
