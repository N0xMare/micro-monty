rows = await query_database({"sql": """
SELECT AVG(units) AS avg_units, COUNT(*) AS n FROM (
  SELECT o.id, COALESCE(SUM(oi.qty), 0) AS units
  FROM orders o
  LEFT JOIN order_items oi ON oi.order_id = o.id
  WHERE o.status = 'completed'
  GROUP BY o.id
)
"""})
print(rows)
avg = round(rows[0]["avg_units"], 2)
print(await submit_answer({"average_units": avg}))
