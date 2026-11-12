import json

currencies = json.loads(await query_database({"sql": "SELECT currency, COUNT(*) AS n FROM orders WHERE status = 'completed' GROUP BY currency"}))
print("completed-order currencies:", currencies)

rows = json.loads(await query_database({"sql": """
    SELECT c.region AS region, SUM(o.total) AS revenue
    FROM orders o JOIN customers c ON c.id = o.customer_id
    WHERE o.status = 'completed'
    GROUP BY c.region
    ORDER BY revenue DESC, c.region
"""}))

lines = ["| Region | Revenue |", "|---|---:|"]
for r in rows:
    lines.append(f"| {r['region']} | ${round(r['revenue'], 2):,.2f} |")
table = "\n".join(lines)

print(table)
print(await post_message({"channel": "#sales", "text": table}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"text": table}))
