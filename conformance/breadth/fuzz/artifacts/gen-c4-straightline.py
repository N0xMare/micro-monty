"""Writes the C4 seed `run_source-slow-straightline-c4` (README, "0.0.1
release campaign on host1"): straight-line code with no safepoint, each
statement a bounded O(len) native op on 10 MB strings. Byte 0 is the
`run_source` target's control byte, as in the other run_source seeds.

    python3 gen-c4-straightline.py [N] > run_source-slow-straightline-c4

N defaults to 2,000 statements (2.36 s with fuel 100 on 5df122e instead of
parking); the campaign's repro used 20,000 (23.2 s past a 50 ms deadline).
"""
import sys

n = int(sys.argv[1]) if len(sys.argv) > 1 else 2000
src = 's = "a" * 10000000\nt = "a" * 10000000\n' + 'b = s == t\n' * n
sys.stdout.buffer.write(b'\0' + src.encode())
