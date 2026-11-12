const rows = await tools.query_database({ sql: `
SELECT c.id AS id, c.name AS name, SUM(o.total) AS total
FROM orders o
JOIN customers c ON c.id = o.customer_id
WHERE o.status = 'completed'
GROUP BY c.id, c.name
ORDER BY total DESC
LIMIT 6
` });
console.log(rows); // 6th row shown to spot ties at the boundary
const top = rows.slice(0, 5).map((r) => ({ name: r.name, total: Number(r.total.toFixed(2)) }));
console.log(await tools.submit_answer({ top }));
