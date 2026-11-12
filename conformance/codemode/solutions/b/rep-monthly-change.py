import json

rows = json.loads(await query_database({"sql": """
    SELECT substr(order_date, 1, 7) AS month, COUNT(*) AS orders, SUM(total) AS revenue
    FROM orders
    WHERE status = 'completed'
    GROUP BY month
    ORDER BY month
"""}))

lines = ["month    orders    revenue   change"]
prev = None
for r in rows:
    rev = r["revenue"]
    if prev is None or prev == 0:
        change = "n/a".rjust(8)
    else:
        change = f"{(rev - prev) / prev * 100:+.1f}%".rjust(8)
    lines.append(f"{r['month']} {r['orders']:>9} {rev:>10,.0f} {change}")
    prev = rev
content = "\n".join(lines) + "\n"

print(content)
print(await write_file({"path": "/out/monthly.txt", "content": content}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"path": "/out/monthly.txt"}))
