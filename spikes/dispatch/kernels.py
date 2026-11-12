"""Spike S1 kernels in Python: the semantics every bytecode variant must match,
the expected results, and the CPython timing baseline.

    ~/.local/bin/python3.12 kernels.py [REPS]       # default 15

Each kernel is a function (so its variables are fast locals, like the VM
kernels' registers/locals). Objects the brief allows outside the timed region
(the `P` and `C` instances) are created before the timer starts, fresh per rep.
Timing: time.perf_counter around the kernel call only (startup excluded).
"""

import statistics
import sys
import time


def fib(n):
    return n if n < 2 else fib(n - 1) + fib(n - 2)


def k_fib():
    return fib(27)


def k_loop():
    s = 0
    for i in range(3_000_000):
        s += i
    return s


def make(k):
    def add(x):
        return x + k

    return add


def k_closure():
    f = make(3)
    s = 0
    for i in range(1_000_000):
        s = f(s)
    return s


class P:
    def __init__(self):
        self.x = 1
        self.y = 2


def k_attr(p):
    s = 0
    for i in range(1_000_000):
        s += p.x + p.y
        p.x = i
    return s


class C:
    def __init__(self):
        self.k = 2

    def inc(self, v):
        return v + self.k


def k_method(c):
    s = 0
    for i in range(1_000_000):
        s = c.inc(s)
    return s


def k_list():
    lst = []
    for i in range(1_000_000):
        lst.append(i)
    s = 0
    for i in range(1_000_000):
        s += lst[i]
    return s


def k_dict():
    d = {}
    for i in range(200_000):
        d[i] = i
    s = 0
    for i in range(200_000):
        s += d[i]
    return s


# name -> (kernel, untimed argument factory)
KERNELS = {
    "fib": (k_fib, lambda: ()),
    "loop": (k_loop, lambda: ()),
    "closure": (k_closure, lambda: ()),
    "attr": (k_attr, lambda: (P(),)),
    "method": (k_method, lambda: (C(),)),
    "list": (k_list, lambda: ()),
    "dict": (k_dict, lambda: ()),
}

# Values CPython 3.12 computes; src/kernels.rs `Kernel::expected` holds the same.
EXPECTED = {
    "fib": 196_418,
    "loop": 4_499_998_500_000,
    "closure": 3_000_000,
    "attr": 500_000_500_002,
    "method": 2_000_000,
    "list": 499_999_500_000,
    "dict": 19_999_900_000,
}


def main():
    reps = int(sys.argv[1]) if len(sys.argv) > 1 else 15
    print(f"# CPython {sys.version.split()[0]} kernels.py reps={reps} (+1 untimed warm-up/verification run)")
    print("engine,kernel,reps,min_ms,median_ms,max_ms,result,expected,ok")
    for name, (fn, args) in KERNELS.items():
        result = fn(*args())  # warm-up, verified below
        assert result == EXPECTED[name], (name, result, EXPECTED[name])
        times = []
        for _ in range(reps):
            a = args()
            t0 = time.perf_counter()
            r = fn(*a)
            times.append((time.perf_counter() - t0) * 1e3)
            assert r == EXPECTED[name], (name, r)
        print(
            f"cpython,{name},{reps},{min(times):.2f},{statistics.median(times):.2f},"
            f"{max(times):.2f},{result},{EXPECTED[name]},{result == EXPECTED[name]}"
        )


if __name__ == "__main__":
    main()
