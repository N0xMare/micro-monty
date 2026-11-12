import json

rows = json.loads(await query_database({"sql": "SELECT substr(order_date, 1, 7) AS month, COUNT(*) AS orders, SUM(total) AS revenue FROM orders WHERE status = 'completed' GROUP BY month ORDER BY month"}))
lines = ["month    orders    revenue   change"]
previous = None
for row in rows:
    revenue = row["revenue"]
    change = "n/a" if previous is None or previous == 0 else f"{(revenue / previous - 1) * 100:+.1f}%"
    lines.append(f"{row['month']} {row['orders']:9d} {revenue:10,.0f} {change:>8}")
    previous = revenue
await write_file({"path": "/out/monthly.txt", "content": "\n".join(lines) + "\n"})
