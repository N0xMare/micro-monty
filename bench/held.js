// held: allocation-heavy work while 20k generators are suspended, then each
// is resumed to the end.
function total(...a) {
  let s = 0;
  for (const x of a) s += x;
  return s;
}

function* plain(i) {
  const row = [i, i + 1, i + 2];
  const n = String(i).repeat(4).length;
  yield n;
  yield row[0] + row[1] + row[2] + n;
}

function* windowed(i) {
  const n = i % 7;
  const sent = yield n;
  yield total(n, sent, 1, 2, 3);
}

const gens = [];
for (let i = 0; i < 10000; i++) {
  const p = plain(i);
  p.next();
  gens.push(p);
  const w = windowed(i);
  w.next();
  gens.push(w);
}

let kept = 0;
for (let r = 0; r < 70; r++) {
  const words = [];
  for (let i = 0; i < 6000; i++) words.push(String(i).repeat(6));
  const index = new Map();
  words.forEach((w, k) => index.set(w, k));
  kept += index.size + words[words.length - 1].length;
}

let resumed = 0;
for (const g of gens) {
  let r = g.next(5);
  while (!r.done) {
    resumed += r.value;
    r = g.next();
  }
}
console.log(kept, resumed);
