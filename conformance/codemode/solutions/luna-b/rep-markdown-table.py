import json

sql = "SELECT c.region, SUM(o.total) AS revenue FROM orders o JOIN customers c ON c.id = o.customer_id WHERE o.status = 'completed' GROUP BY c.region ORDER BY revenue DESC, c.region"
rows = json.loads(await query_database({"sql": sql}))
lines = ["| Region | Revenue |", "|---|---:|"]
lines.extend(f"| {row['region']} | ${row['revenue']:,.2f} |" for row in rows)
await post_message({"channel": "#sales", "text": "\n".join(lines)})
