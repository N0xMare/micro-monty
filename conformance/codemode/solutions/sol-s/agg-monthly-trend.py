rows = await query_database({"sql": "SELECT substr(order_date, 1, 7) AS month, ROUND(SUM(total), 2) AS total FROM orders WHERE status = 'completed' AND order_date >= '2025-10-01' AND order_date < '2026-04-01' GROUP BY substr(order_date, 1, 7) ORDER BY month"})
totals = {r["month"]: r["total"] for r in rows}
months = ["2025-10", "2025-11", "2025-12", "2026-01", "2026-02", "2026-03"]
await submit_answer({"months": [{"month": m, "total": totals.get(m, 0.0)} for m in months]})
