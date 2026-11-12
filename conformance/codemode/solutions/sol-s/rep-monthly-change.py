rows = await query_database({"sql": "SELECT substr(order_date, 1, 7) AS month, COUNT(*) AS orders, SUM(total) AS revenue FROM orders WHERE status = 'completed' GROUP BY month ORDER BY month"})
lines = ["month    orders    revenue   change"]
previous = None
for r in rows:
    change = "n/a" if previous is None or previous == 0 else f"{(r['revenue'] / previous - 1) * 100:+.1f}%"
    lines.append(f"{r['month']} {r['orders']:9d} {r['revenue']:10,.0f} {change:>8}")
    previous = r["revenue"]
await write_file({"path": "/out/monthly.txt", "content": "\n".join(lines)})
