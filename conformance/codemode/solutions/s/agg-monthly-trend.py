rows = await query_database({"sql": """
SELECT substr(order_date, 1, 7) AS month, SUM(total) AS total
FROM orders
WHERE status = 'completed' AND order_date >= '2025-10-01' AND order_date <= '2026-03-31'
GROUP BY month
ORDER BY month
"""})
by_month = {r["month"]: r["total"] or 0 for r in rows}
wanted = ["2025-10", "2025-11", "2025-12", "2026-01", "2026-02", "2026-03"]
months = [{"month": m, "total": round(by_month.get(m, 0.0), 2)} for m in wanted]
print(months)
print(await submit_answer({"months": months}))
