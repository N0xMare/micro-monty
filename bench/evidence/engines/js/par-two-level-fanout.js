const ids = Array.from({ length: 10 }, (_, i) => 1100 + i);
const orders = await Promise.all(ids.map((id) => tools.get_order({ order_id: id })));

const productIds = [...new Set(orders.flatMap((o) => o.items.map((item) => item.product_id)))].sort((a, b) => a - b);
const products = await Promise.all(productIds.map((pid) => tools.get_product({ product_id: pid })));
const categoryOf = new Map(products.map((p) => [p.id, p.category]));

const units = {};
for (const o of orders) {
  for (const item of o.items) {
    const c = categoryOf.get(item.product_id);
    units[c] = (units[c] ?? 0) + item.qty;
  }
}
const sorted = Object.fromEntries(Object.entries(units).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
console.log(sorted);
await tools.submit_answer({ units: sorted });
