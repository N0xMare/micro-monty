async function allCustomers() {
  const first = await tools.list_customers({ page: 1 });
  const customers = [...first.customers];
  const pages = first.total_pages ?? 1;
  if (pages > 1) {
    const rest = await Promise.all(Array.from({ length: pages - 1 }, (_, i) => tools.list_customers({ page: i + 2 })));
    for (const r of rest) customers.push(...r.customers);
  }
  return customers;
}

async function allCompletedOrders() {
  const orders = [];
  let cursor = null;
  while (true) {
    const args = { status: "completed" };
    if (cursor) args.cursor = cursor;
    const res = await tools.list_orders(args);
    orders.push(...res.orders);
    cursor = res.next_cursor;
    if (!cursor) break;
  }
  return orders;
}

const [customers, orders] = await Promise.all([allCustomers(), allCompletedOrders()]);
const tierOf = new Map(customers.map((c) => [c.id, c.tier]));
const seen = new Map();
for (const o of orders) seen.set(o.id, o);
const sums = {};
const counts = {};
for (const o of seen.values()) {
  if (o.status !== "completed") continue;
  const t = tierOf.get(o.customer_id);
  if (t === undefined) continue;
  sums[t] = (sums[t] ?? 0) + o.total;
  counts[t] = (counts[t] ?? 0) + 1;
}
const averages = {};
for (const t of ["bronze", "silver", "gold"]) if (counts[t]) averages[t] = Number((sums[t] / counts[t]).toFixed(2));
console.log(averages, counts);
console.log(await tools.submit_answer({ averages }));
