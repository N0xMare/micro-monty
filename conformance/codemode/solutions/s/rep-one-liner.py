rows = await query_database({"sql": "SELECT status, COUNT(*) AS n FROM orders GROUP BY status"})
counts = {r["status"]: r["n"] for r in rows}
total = sum(counts.values())

def pct(s):
    return round(counts.get(s, 0) / total * 100) if total else 0

text = (f"Orders: {total} (completed {pct('completed')}%, pending {pct('pending')}%, "
        f"cancelled {pct('cancelled')}%, refunded {pct('refunded')}%)")
print(text)

res = await post_message({"channel": "#sales", "text": text})
print(res)
