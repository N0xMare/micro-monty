import json

rows = json.loads(await query_database({
    "sql": "SELECT strftime('%Y-%m', order_date) AS month, COUNT(*) AS orders, SUM(total) AS revenue FROM orders WHERE status = 'completed' GROUP BY strftime('%Y-%m', order_date) ORDER BY month"
}))
lines = ["month    orders    revenue   change"]
previous = None
for row in rows:
    revenue = row["revenue"] or 0
    if previous is None or previous == 0:
        change = "n/a"
    else:
        change = f"{(revenue - previous) / previous * 100:+.1f}%"
    lines.append(f"{row['month']} {row['orders']:>9} {revenue:>10,.0f} {change:>8}")
    previous = revenue
await write_file({"path": "/out/monthly.txt", "content": "\n".join(lines) + "\n"})
