// dictint: insert 200k int keys, then look each one up.
const d = new Map();
for (let i = 0; i < 200000; i++) {
  d.set(i, i);
}
let total = 0;
for (let i = 0; i < 200000; i++) {
  total += d.get(i);
}
const n = d.size;
console.log(n, total);
