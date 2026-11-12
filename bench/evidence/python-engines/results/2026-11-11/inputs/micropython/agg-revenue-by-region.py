async def __codemode_main():
    global rows, totals
    rows = await query_database({"sql": """
SELECT c.region AS region, SUM(o.total) AS total
FROM orders o
JOIN customers c ON c.id = o.customer_id
WHERE o.status = 'completed'
  AND o.order_date >= '2026-02-01' AND o.order_date < '2026-03-01'
GROUP BY c.region
ORDER BY c.region
"""})
    print(rows)
    totals = {r["region"]: round(r["total"], 2) for r in rows if r["total"]}
    print(await submit_answer({"totals": totals}))
    pass
import asyncio as __codemode_asyncio
__codemode_asyncio.run(__codemode_main())
