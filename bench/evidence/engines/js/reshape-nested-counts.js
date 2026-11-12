const first = await tools.list_customers({ page: 1 });
const total = first.total_pages;
const rest = await Promise.all(Array.from({ length: total - 1 }, (_, i) => tools.list_customers({ page: i + 2 })));
const customers = [];
const seen = new Set();
for (const page of [first, ...rest]) {
  for (const c of page.customers) {
    if (seen.has(c.id)) continue;
    seen.add(c.id);
    customers.push(c);
  }
}
const nested = {};
for (const c of customers) {
  nested[c.region] ??= {};
  nested[c.region][c.tier] = (nested[c.region][c.tier] ?? 0) + 1;
}
const byKey = (a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0);
const counts = Object.fromEntries(
  Object.entries(nested).sort(byKey).map(([r, t]) => [r, Object.fromEntries(Object.entries(t).sort(byKey))]),
);
console.log(customers.length, counts);
console.log(await tools.submit_answer({ counts }));
