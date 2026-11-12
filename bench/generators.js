// generators: a three-stage generator pipeline, `yield*`, and generator
// functions standing in for Python's generator expressions.
function* numbers(n) {
  let i = 0;
  while (i < n) {
    yield i;
    i += 1;
  }
}

function* evens(source) {
  for (const x of source) {
    if (x % 2 === 0) {
      yield x;
    }
  }
}

function* scaled(source, k) {
  for (const x of source) {
    yield x * k;
  }
}

function* range(start, stop) {
  for (let i = start; i < stop; i++) {
    yield i;
  }
}

function* chunks(n, size) {
  for (let start = 0; start < n; start += size) {
    yield* range(start, Math.min(start + size, n));
  }
}

function sum(source) {
  let total = 0;
  for (const x of source) {
    total += x;
  }
  return total;
}

function any(source) {
  for (const x of source) {
    if (x) {
      return true;
    }
  }
  return false;
}

let pipeline = 0;
for (const x of scaled(evens(numbers(300000)), 3)) {
  pipeline += x;
}

const squares = sum(
  (function* () {
    for (let x = 0; x < 300000; x++) yield (x * x) % 1000;
  })(),
);
const chunked = sum(
  (function* () {
    for (const x of chunks(100000, 64)) yield x % 7;
  })(),
);
let found = 0;
for (let limit = 0; limit < 100; limit++) {
  if (
    any(
      (function* () {
        for (let x = 0; x < 10000; x++) yield x > limit * 100;
      })(),
    )
  ) {
    found += 1;
  }
}
console.log(pipeline, squares, chunked, found);
