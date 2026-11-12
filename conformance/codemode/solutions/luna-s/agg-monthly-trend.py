rows = await query_database({"sql": "SELECT substr(order_date, 1, 7) AS month, SUM(total) AS total FROM orders WHERE status = 'completed' AND order_date >= '2025-10-01' AND order_date < '2026-04-01' GROUP BY month"})
totals = {row["month"]: round(float(row["total"]), 2) for row in rows}
months = [{"month": f"{year}-{month:02d}", "total": totals.get(f"{year}-{month:02d}", 0.0)} for year, month in ((2025, 10), (2025, 11), (2025, 12), (2026, 1), (2026, 2), (2026, 3))]
await submit_answer({"months": months})
