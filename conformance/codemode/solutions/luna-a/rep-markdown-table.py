import json

rows = json.loads(await query_database({
    "sql": "SELECT c.region, SUM(o.total) AS revenue FROM orders o JOIN customers c ON c.id = o.customer_id WHERE o.status = 'completed' GROUP BY c.region"
}))
rows.sort(key=lambda row: (-row["revenue"], row["region"]))
text = "| Region | Revenue |\n|---|---:|\n" + "\n".join(f"| {row['region']} | ${row['revenue']:,.2f} |" for row in rows)
await post_message({"channel": "#sales", "text": text})
