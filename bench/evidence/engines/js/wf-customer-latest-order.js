const targetEmail = "ben.kim23@example.com";
let customer = null;
let page = 1;
while (customer === null) {
  const res = await tools.list_customers({ page });
  for (const c of res.customers) {
    if (c.email.trim().toLowerCase() === targetEmail) {
      customer = c;
      break;
    }
  }
  if (page >= res.total_pages) break;
  page += 1;
}
if (customer === null) throw new Error("customer not found");

const orders = [];
let cursor = null;
while (true) {
  const args = { customer_id: customer.id };
  if (cursor) args.cursor = cursor;
  const res = await tools.list_orders(args);
  orders.push(...res.orders);
  cursor = res.next_cursor;
  if (!cursor) break;
}

let latest = orders[0];
for (const o of orders) {
  if (o.order_date > latest.order_date || (o.order_date === latest.order_date && o.id > latest.id)) latest = o;
}
console.log(customer, latest);
let action = "none";
if (latest.status === "pending") {
  await tools.send_email({
    to: customer.email,
    subject: `Your order ${latest.id} is still pending`,
    body: `Hello ${customer.name},\n\nYour order ${latest.id} placed on ${latest.order_date} is still pending. We'll let you know as soon as it moves forward.\n\nThank you for your patience.`,
  });
  action = "emailed";
}
await tools.submit_answer({ order_id: latest.id, action });
