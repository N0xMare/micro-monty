const ids = Array.from({ length: 40 }, (_, i) => 1001 + i);
const orders = await Promise.all(ids.map((id) => tools.get_order({ order_id: id })));
let sum = 0;
for (const o of orders) sum += o.total;
const total = Number(sum.toFixed(2));
const refunded = orders.filter((o) => String(o.status ?? "").toLowerCase() === "refunded").length;
const currencies = new Set(orders.map((o) => o.currency));
console.log(`orders=${orders.length} total=${total} refunded=${refunded} currencies=${[...currencies]}`);
console.log(await tools.submit_answer({ total, refunded }));
