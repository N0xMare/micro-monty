// loop: a module-level counting loop, summing range(3_000_000).
let total = 0;
for (let i = 0; i < 3000000; i++) {
  total += i;
}
console.log(total);
