import json

rows = json.loads(await query_database({"sql": """
    SELECT substr(order_date, 1, 7) AS month, COUNT(*) AS orders, SUM(total) AS revenue
    FROM orders WHERE status = 'completed'
    GROUP BY month ORDER BY month"""}))
lines = ["month    orders    revenue   change"]
prev = None
for r in rows:
    if prev is None or not prev:
        change = "n/a".rjust(8)
    else:
        change = f"{(r['revenue'] - prev) / prev * 100:+.1f}%".rjust(8)
    lines.append(f"{r['month']} {r['orders']:>9} {r['revenue']:>10,.0f} {change}")
    prev = r["revenue"]
content = "\n".join(lines) + "\n"
print(content)
print(await write_file({"path": "/out/monthly.txt", "content": content}))
