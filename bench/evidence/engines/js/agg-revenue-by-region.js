const rows = await tools.query_database({ sql: `
SELECT c.region AS region, SUM(o.total) AS total
FROM orders o
JOIN customers c ON c.id = o.customer_id
WHERE o.status = 'completed'
  AND o.order_date >= '2026-02-01' AND o.order_date < '2026-03-01'
GROUP BY c.region
ORDER BY c.region
` });
console.log(rows);
const totals = {};
for (const r of rows) if (r.total) totals[r.region] = Number(r.total.toFixed(2));
console.log(await tools.submit_answer({ totals }));
