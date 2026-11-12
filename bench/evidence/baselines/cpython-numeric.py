"""Same bounded scalar sum; compile/definition/calls timed independently."""
import json
import sys
import time

n, repetitions = map(int, sys.argv[1:])
assert 0 <= n <= 10_000_000 and 1 <= repetitions <= 1000
start = time.perf_counter_ns()
namespace = {}
setup_ns = time.perf_counter_ns() - start
start = time.perf_counter_ns()
code = compile(
    "def numeric_loop(n):\n    total = 0\n    for i in range(n):\n        total += i\n    return total\n",
    "numeric.py", "exec",
)
compile_ns = time.perf_counter_ns() - start
start = time.perf_counter_ns()
exec(code, namespace)
initialize_ns = time.perf_counter_ns() - start
function = namespace["numeric_loop"]
expected = n * (n - 1) // 2
samples = []
for _ in range(repetitions + 1):
    start = time.perf_counter_ns()
    actual = function(n)
    elapsed = time.perf_counter_ns() - start
    assert actual == expected
    samples.append(elapsed)
print(json.dumps(dict(
    n=n, checked_result=expected, setup_ns=setup_ns, compile_ns=compile_ns,
    initialize_ns=initialize_ns, first_ns=samples[0], warm_ns=samples[1:],
)))
