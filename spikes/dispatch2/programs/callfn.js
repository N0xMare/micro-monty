// callfn: 1M calls of a plain global function from inside a function.
function inc(x) {
  return x + 1;
}

function main() {
  let total = 0;
  for (let i = 0; i < 1000000; i++) {
    total = inc(total);
  }
  return total;
}

console.log(main());
