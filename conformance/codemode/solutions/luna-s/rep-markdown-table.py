rows = await query_database({"sql": "SELECT c.region AS region, SUM(o.total) AS revenue FROM orders o JOIN customers c ON c.id = o.customer_id WHERE o.status = 'completed' GROUP BY c.region ORDER BY revenue DESC, c.region"})
lines = ["| Region | Revenue |", "|---|---:|"]
lines.extend(f"| {r['region']} | ${r['revenue']:,.2f} |" for r in rows)
await post_message({"channel": "#sales", "text": "\n".join(lines)})
