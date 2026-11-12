const ids = Array.from({ length: 12 }, (_, i) => "A" + String(i + 1).padStart(3, "0"));
const results = await Promise.allSettled(ids.map((id) => tools.get_transactions({ account_id: id })));

const lows = {};
ids.forEach((a, i) => {
  const r = results[i];
  if (r.status === "rejected") {
    console.log(a, "error:", r.reason.message);
    return;
  }
  let running = 0;
  let low = 0;
  for (const t of r.value) {
    running += t.amount;
    low = Math.min(low, running);
  }
  lows[a] = low;
  console.log(a, r.value.length, Number(low.toFixed(2)));
});
let deepest = null;
for (const a of Object.keys(lows)) if (deepest === null || lows[a] < lows[deepest]) deepest = a;
await tools.submit_answer({ account_id: deepest, low: Number(lows[deepest].toFixed(2)) });
