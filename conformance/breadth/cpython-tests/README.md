# CPython's own test suite on micro-monty

Counts how much of CPython 3.14.7's `Lib/test` passes on micro-monty, for the
modules that cover the language core and the bundled stdlib. The unit is the
test method; a method counts as passing on mm only if it passes on CPython
with the same shim (`shim/`) and the same transformed source. `modules.txt`
lists the units. No gate reads these files.

`results/` holds the 2026-11-09 run [M]: `summary.md` (per-module table and
top failure causes), `units.jsonl` and `methods.jsonl`.

## Commands

```bash
# 1. CPython 3.14.7 Lib/test, in its own empty directory
mkdir -p /tmp/cpython-src-3147 && cd /tmp/cpython-src-3147
curl -sSfLO https://www.python.org/ftp/python/3.14.7/Python-3.14.7.tgz
curl -sSfLO https://www.python.org/ftp/python/3.14.7/Python-3.14.7.tgz.sigstore
sha256sum Python-3.14.7.tgz   # 62859805...2dbe07
uvx sigstore verify identity --bundle Python-3.14.7.tgz.sigstore \
  --cert-identity hugo@python.org --cert-oidc-issuer https://github.com/login/oauth Python-3.14.7.tgz
mkdir x && tar -xzf Python-3.14.7.tgz -C x Python-3.14.7/Lib/test

# 2. run (about 4 minutes wall with 4 jobs; at most 4 engine processes at a time)
cd conformance/breadth/cpython-tests
LIBTEST=/tmp/cpython-src-3147/x/Python-3.14.7/Lib/test
~/.local/bin/python3.14 -I harness.py --lib-test $LIBTEST \
  --mm ../../../target/release/mm --jobs 4 --out /tmp/mm-cpython-tests/full
#    --only test_bool test_list    runs a subset

# 3. summarize into results/
~/.local/bin/python3.14 -I report.py /tmp/mm-cpython-tests/full --lib-test $LIBTEST
```

`harness.py` writes generated sources and per-process output to
`/tmp/mm-cpython-tests/work/<unit>/` (`$MM_CPYTHON_TESTS_WORK` overrides it).
Rerun a single file by hand with `mm run work/<unit>/generated-mm-0.py` or
`python3.14 -I work/<unit>/generated.py`. Result lines go to stderr.
