// floats: **, negation and comparisons on doubles. The values are dyadic
// (n / 8, n / 32) and stay exact, so every engine prints the same integers.
let above = 0;
let negatives = 0;
let acc = 0.0;
let powers = 0.0;
for (let i = 1; i <= 200000; i++) {
  const x = (i % 1000) / 8;
  const y = -x;
  const sq = x ** 2;
  const cu = (x / 4) ** 3;
  if (sq > cu) {
    above += 1;
  }
  if (y <= -50.0) {
    negatives += 1;
  }
  acc += sq - cu;
  powers += 2.0 ** (i % 20) - -y;
}
console.log(above, negatives, Math.trunc(acc * 32768), Math.trunc(powers));
