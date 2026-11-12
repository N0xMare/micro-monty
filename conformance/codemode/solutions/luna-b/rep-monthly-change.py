import json

sql = "SELECT strftime('%Y-%m', order_date) AS month, COUNT(*) AS orders, SUM(total) AS revenue FROM orders WHERE status = 'completed' GROUP BY month ORDER BY month"
rows = json.loads(await query_database({"sql": sql}))
lines = ["month    orders    revenue   change"]
previous = None
for row in rows:
    revenue = row["revenue"]
    change = "     n/a" if previous is None or previous == 0 else f"{(revenue - previous) / previous * 100:+7.1f}%"
    lines.append(f"{row['month']} {row['orders']:>9} {revenue:>10,.0f} {change}")
    previous = revenue
await write_file({"path": "/out/monthly.txt", "content": "\n".join(lines) + "\n"})
