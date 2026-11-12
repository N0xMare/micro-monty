rows = await query_database({"sql": "SELECT strftime('%Y-%m', order_date) AS month, COUNT(*) AS orders, SUM(total) AS revenue FROM orders WHERE status = 'completed' GROUP BY strftime('%Y-%m', order_date) ORDER BY month"})
lines = ["month    orders    revenue   change"]
previous = None
for r in rows:
    change = "     n/a" if previous is None else f"{((r['revenue'] - previous) / previous * 100 if previous else 0):+7.1f}%"
    lines.append(f"{r['month']} {r['orders']:>9} {r['revenue']:>10,.0f} {change:>8}")
    previous = r["revenue"]
await write_file({"path": "/out/monthly.txt", "content": "\n".join(lines) + "\n"})
