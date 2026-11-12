// closure: 1M calls of a closure that reads a captured variable.
function makeAdder(n) {
  return function add(x) {
    return x + n;
  };
}

const add = makeAdder(3);
let total = 0;
for (let i = 0; i < 1000000; i++) {
  total = add(total);
}
console.log(total);
