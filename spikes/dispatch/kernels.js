// Spike S1 kernels in JavaScript, for QuickJS (dquickts `qjs`).
//
//     /tmp/dquickts/qjs kernels.js [REPS]       // default 15
//
// Same structure as kernels.py: each kernel is a function with locals; the P
// and C instances are created before the timer starts, fresh per rep.
// Differences forced by the language: `for (let i = 0; i < n; i++)` replaces
// `range`; `Map` replaces the int-keyed dict; QuickJS numbers are int32 or
// float64, so sums past 2^31 (loop, attr, list, dict) continue in float64.
// Timing: performance.now() around the kernel call only.

function fib(n) {
  return n < 2 ? n : fib(n - 1) + fib(n - 2);
}

function k_fib() {
  return fib(27);
}

function k_loop() {
  let s = 0;
  for (let i = 0; i < 3000000; i++) s += i;
  return s;
}

function make(k) {
  return function add(x) {
    return x + k;
  };
}

function k_closure() {
  const f = make(3);
  let s = 0;
  for (let i = 0; i < 1000000; i++) s = f(s);
  return s;
}

class P {
  constructor() {
    this.x = 1;
    this.y = 2;
  }
}

function k_attr(p) {
  let s = 0;
  for (let i = 0; i < 1000000; i++) {
    s += p.x + p.y;
    p.x = i;
  }
  return s;
}

class C {
  constructor() {
    this.k = 2;
  }
  inc(v) {
    return v + this.k;
  }
}

function k_method(c) {
  let s = 0;
  for (let i = 0; i < 1000000; i++) s = c.inc(s);
  return s;
}

function k_list() {
  const lst = [];
  for (let i = 0; i < 1000000; i++) lst.push(i);
  let s = 0;
  for (let i = 0; i < 1000000; i++) s += lst[i];
  return s;
}

function k_dict() {
  const d = new Map();
  for (let i = 0; i < 200000; i++) d.set(i, i);
  let s = 0;
  for (let i = 0; i < 200000; i++) s += d.get(i);
  return s;
}

const KERNELS = [
  ["fib", k_fib, () => []],
  ["loop", k_loop, () => []],
  ["closure", k_closure, () => []],
  ["attr", k_attr, () => [new P()]],
  ["method", k_method, () => [new C()]],
  ["list", k_list, () => []],
  ["dict", k_dict, () => []],
];

const EXPECTED = {
  fib: 196418,
  loop: 4499998500000,
  closure: 3000000,
  attr: 500000500002,
  method: 2000000,
  list: 499999500000,
  dict: 19999900000,
};

function median(xs) {
  const v = xs.slice().sort((a, b) => a - b);
  const n = v.length;
  return n % 2 ? v[(n - 1) / 2] : (v[n / 2 - 1] + v[n / 2]) / 2;
}

const args = typeof scriptArgs !== "undefined" ? scriptArgs : [];
const reps = args.length > 1 ? parseInt(args[1], 10) : 15;
console.log(`# qjs kernels.js reps=${reps} (+1 untimed warm-up/verification run)`);
console.log("engine,kernel,reps,min_ms,median_ms,max_ms,result,expected,ok");
let allOk = true;
for (const [name, fn, mkArgs] of KERNELS) {
  const result = fn(...mkArgs());
  const ok = result === EXPECTED[name];
  allOk = allOk && ok;
  const times = [];
  for (let r = 0; r < reps; r++) {
    const a = mkArgs();
    const t0 = performance.now();
    const v = fn(...a);
    times.push(performance.now() - t0);
    allOk = allOk && v === EXPECTED[name];
  }
  console.log(
    `qjs,${name},${reps},${Math.min(...times).toFixed(2)},${median(times).toFixed(2)},` +
      `${Math.max(...times).toFixed(2)},${result},${EXPECTED[name]},${ok}`,
  );
}
if (!allOk) throw new Error("result mismatch");
