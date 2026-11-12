// exceptions: throw and catch in a hot loop: a builtin error thrown by a
// called function, a custom subclass carrying a field, try/finally with an
// else flag, and a thrown error for a missing Map key (Python's KeyError).
class Invalid extends RangeError {
  constructor(code) {
    super(String(code));
    this.code = code;
  }
}

class KeyError extends Error {}

function check(i) {
  if (i % 3 === 0) {
    throw new RangeError('multiple of three');
  }
  return i;
}

function validate(i) {
  if (i % 5 === 0) {
    throw new Invalid(i % 11);
  }
  return 1;
}

function lookup(map, key) {
  const value = map.get(key);
  if (value === undefined) {
    throw new KeyError(String(key));
  }
  return value;
}

let caught = 0;
let codes = 0;
let passed = 0;
let cleanups = 0;
let missing = 0;
const table = new Map();
for (let i = 0; i < 1000; i += 2) {
  table.set(i, i);
}
for (let i = 0; i < 100000; i++) {
  try {
    passed += check(i) % 2;
  } catch (error) {
    if (!(error instanceof RangeError)) throw error;
    caught += 1;
  }
  let ok = false;
  try {
    validate(i);
    ok = true;
  } catch (error) {
    if (!(error instanceof Invalid)) throw error;
    codes += error.code;
  } finally {
    cleanups += 1;
  }
  if (ok) {
    passed += 1;
  }
  try {
    passed += lookup(table, i % 1000);
  } catch (error) {
    if (!(error instanceof KeyError)) throw error;
    missing += 1;
  }
}
console.log(caught, codes, passed, cleanups, missing);
