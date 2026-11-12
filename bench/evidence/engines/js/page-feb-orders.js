const counts = {};
const seen = new Set();
let cursor = null;
while (true) {
  const args = { since: "2026-02-01", until: "2026-02-28" };
  if (cursor) args.cursor = cursor;
  const resp = await tools.list_orders(args);
  for (const o of resp.orders) {
    if (o.order_date.startsWith("2026-02") && !seen.has(o.id)) {
      seen.add(o.id);
      counts[o.status] = (counts[o.status] ?? 0) + 1;
    }
  }
  cursor = resp.next_cursor;
  if (!cursor) break;
}
console.log(counts);
await tools.submit_answer({ counts });
