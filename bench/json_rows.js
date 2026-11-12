// json: serialise 50k rows to text, then parse them back.
const rows = [];
for (let i = 0; i < 50000; i++) {
  rows.push({ id: i, name: 'user' + String(i), active: i % 2 === 0, tags: ['a', 'b'], score: i * 3 });
}
const text = JSON.stringify(rows);
const back = JSON.parse(text);
let total = 0;
for (const row of back) {
  total += row.score;
}
console.log(text.length, back.length, total);
