// loopfn: callfn's loop with the call replaced by its body.
function main() {
  let total = 0;
  for (let i = 0; i < 1000000; i++) {
    total = total + 1;
  }
  return total;
}

console.log(main());
