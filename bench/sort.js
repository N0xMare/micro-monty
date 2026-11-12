// sort: array building (filtered, nested, over tuples) and sorting 50k records
// with comparators standing in for Python's key functions: one field, a
// two-field key, reverse, and a stable re-sort on a key with many ties.
const records = [];
for (let i = 0; i < 50000; i++) {
  records.push([(i * 7919) % 50000, 'n' + String((i * 31) % 1000), i % 10]);
}
const byValue = records.slice().sort((a, b) => a[0] - b[0]);
const byName = records
  .slice()
  .sort((a, b) => (a[1] < b[1] ? -1 : a[1] > b[1] ? 1 : b[0] - a[0]));
const byBucket = byValue.slice().sort((a, b) => b[2] - a[2]);
const names = byName.filter((r) => r[2] < 5).map((r) => r[1]);
const words = [...new Set(names)].sort((a, b) => a.length - b.length);
const grid = [];
for (let y = 0; y < 100; y++) {
  const row = [];
  for (let x = 0; x < 100; x++) {
    row.push((x * y) % 10);
  }
  grid.push(row);
}
const flat = [];
for (const row of grid) {
  for (const v of row) {
    if (v) flat.push(v);
  }
}
flat.sort((a, b) => a - b);

let check = 0;
byValue.forEach((r, i) => {
  check = (check * 31 + r[0] + i) % 1000000007;
});
byName.slice(0, 1000).forEach((r, i) => {
  check = (check * 31 + r[0] * (i + 1)) % 1000000007;
});
byBucket.forEach((r) => {
  check = (check * 31 + r[0] + r[2]) % 1000000007;
});
let sampled = 0;
for (let i = 0; i < flat.length; i += 97) {
  sampled += flat[i];
}
console.log(check, names.length, words.length, words[0], words[words.length - 1], flat.length, sampled);
