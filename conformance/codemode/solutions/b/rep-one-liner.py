import json, math

rows = json.loads(await query_database({"sql": "SELECT status, COUNT(*) AS n FROM orders GROUP BY status"}))
counts = {r["status"]: r["n"] for r in rows}
total = sum(counts.values())

def pct(status):
    return math.floor(counts.get(status, 0) / total * 100 + 0.5) if total else 0

parts = ", ".join(f"{s} {pct(s)}%" for s in ["completed", "pending", "cancelled", "refunded"])
line = f"Orders: {total} ({parts})"

print(counts)
print(line)
print(await post_message({"channel": "#sales", "text": line}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"text": line}))
