const symbols = ["ACME", "GLBX", "INIT", "UMBR", "WAYN", "STRK", "OSCP", "CYDN"].sort();
const quotes = await Promise.all(symbols.map((symbol) => tools.get_quote({ symbol })));

const lines = ["symbol,price,change"];
symbols.forEach((s, i) => {
  const q = quotes[i];
  const change = (q.change_pct >= 0 ? "+" : "") + q.change_pct.toFixed(2);
  lines.push(`${s},${q.price.toFixed(2)},${change}%`);
});
const content = lines.join("\n") + "\n";
console.log(content);

const res = await tools.write_file({ path: "/out/quotes.csv", content });
console.log(res);
