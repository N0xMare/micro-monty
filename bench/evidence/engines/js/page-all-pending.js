const orders = [];
let cursor = null;
while (true) {
  const args = { status: "pending" };
  if (cursor) args.cursor = cursor;
  const resp = await tools.list_orders(args);
  orders.push(...resp.orders);
  cursor = resp.next_cursor;
  if (!cursor) break;
}
// dedupe by id just in case
const byId = new Map(orders.filter((o) => o.status === "pending").map((o) => [o.id, o]));
const count = byId.size;
let sum = 0;
for (const o of byId.values()) sum += o.total;
const total = Number(sum.toFixed(2));
console.log(count, total);
await tools.submit_answer({ count, total });
