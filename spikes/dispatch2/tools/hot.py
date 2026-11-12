#!/usr/bin/env python3
"""Per-instruction callgrind costs joined with objdump, for one function.

usage: tools/hot.py <callgrind.out (--dump-instr=yes)> <unstripped binary> [function substring] [min count]
Prints the function's disassembly with the instructions executed per line.
"""
import re
import subprocess
import sys
from collections import defaultdict

out, binary = sys.argv[1], sys.argv[2]
func = sys.argv[3] if len(sys.argv) > 3 else "Vm>::run"
minc = int(sys.argv[4]) if len(sys.argv) > 4 else 1

cost = defaultdict(int)
positions = ["line"]
last = [0, 0]
in_calls = 0


def parse_pos(tok, i):
    if tok == "*":
        return last[i]
    if tok[0] in "+-":
        return last[i] + int(tok, 0)
    return int(tok, 0)


for line in open(out):
    line = line.rstrip("\n")
    if line.startswith("positions:"):
        positions = line.split()[1:]
        continue
    if line.startswith("calls="):
        in_calls = 1
        continue
    if not line or not (line[0].isdigit() or line[0] in "+-*"):
        continue
    parts = line.split()
    n = len(positions)
    pos = [parse_pos(parts[i], i) for i in range(n)]
    for i in range(n):
        last[i] = pos[i]
    if in_calls:
        # The line after calls= is the call cost (inclusive): skip it.
        in_calls = 0
        continue
    if len(parts) > n:
        cost[pos[0]] += int(parts[n])

nm = subprocess.run(["nm", "-S", "-C", binary], capture_output=True, text=True).stdout
start = size = None
for l in nm.splitlines():
    f = l.split(maxsplit=3)
    if len(f) == 4 and func in f[3] and f[2] in "tT":
        start, size = int(f[0], 16), int(f[1], 16)
        break
dis = subprocess.run(
    ["objdump", "-d", "--no-show-raw-insn", "-C", binary,
     f"--start-address={start:#x}", f"--stop-address={start + size:#x}"],
    capture_output=True, text=True).stdout
total = 0
for l in dis.splitlines():
    m = re.match(r"\s+([0-9a-f]+):\s+(.*)", l)
    if not m:
        continue
    a = int(m.group(1), 16)
    c = cost.get(a, 0)
    total += c
    if c >= minc:
        print(f"{c:>12,} {a:x}  {m.group(2)}")
print(f"{total:>12,} total in {func} ({size} bytes)")
