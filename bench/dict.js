// dict: insert 200k string keys, then look each one up.
const keys = [];
for (let i = 0; i < 200000; i++) {
  keys.push('key' + String(i));
}
const d = new Map();
let i = 0;
for (const k of keys) {
  d.set(k, i);
  i += 1;
}
let total = 0;
for (const k of keys) {
  total += d.get(k);
}
console.log(d.size, total);
