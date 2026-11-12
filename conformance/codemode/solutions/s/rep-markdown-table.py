rows = await query_database({"sql": """
    SELECT c.region AS region, SUM(o.total) AS revenue
    FROM orders o JOIN customers c ON c.id = o.customer_id
    WHERE o.status = 'completed'
    GROUP BY c.region
    ORDER BY revenue DESC, c.region
"""})

lines = ["| Region | Revenue |", "|---|---:|"]
for r in rows:
    lines.append(f"| {r['region']} | ${r['revenue']:,.2f} |")
text = "\n".join(lines)
print(text)

res = await post_message({"channel": "#sales", "text": text})
print(res)
