const first = await tools.list_accounts({ offset: 0, limit: 10 });
const total = first.total;
const offsets = [];
for (let off = 10; off < total; off += 10) offsets.push(off);
const rest = await Promise.all(offsets.map((off) => tools.list_accounts({ offset: off, limit: 10 })));
const accounts = [...first.accounts];
for (const page of rest) accounts.push(...page.accounts);
console.log(`fetched ${accounts.length} of ${total}`);

const seen = new Set();
const unique = [];
for (const a of accounts) {
  if (!seen.has(a.account_id)) {
    seen.add(a.account_id);
    unique.push(a);
  }
}
const frozen = unique.filter((a) => a.status === "frozen").map((a) => a.account_id);
let usd = 0;
for (const a of unique) if (a.status === "active" && a.currency === "USD") usd += a.balance;
const activeUsdTotal = Number(usd.toFixed(2));
console.log("frozen:", frozen);
console.log("active USD total:", activeUsdTotal);
console.log(await tools.submit_answer({ frozen, active_usd_total: activeUsdTotal }));
