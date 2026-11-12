async function allOrders(customerId) {
  const orders = [];
  let cursor = null;
  while (true) {
    const args = { customer_id: customerId };
    if (cursor) args.cursor = cursor;
    const res = await tools.list_orders(args);
    orders.push(...res.orders);
    cursor = res.next_cursor;
    if (!cursor) break;
  }
  return orders;
}

const [customer, all] = await Promise.all([tools.get_customer({ customer_id: 17 }), allOrders(17)]);
const orders = [...new Map(all.filter((o) => o.customer_id === 17).map((o) => [o.id, o])).values()];
const counts = {};
let completedTotal = 0;
for (const o of orders) {
  counts[o.status] = (counts[o.status] ?? 0) + 1;
  if (o.status === "completed") completedTotal += o.total;
}
const answer = { name: customer.name, counts, completed_total: Number(completedTotal.toFixed(2)) };
console.log(answer);
console.log(await tools.submit_answer(answer));
