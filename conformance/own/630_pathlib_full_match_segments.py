from pathlib import PurePosixPath

p = PurePosixPath('src/pkg/main.py')
assert not p.full_match('src/*.py')
assert p.full_match('src/**/*.py')
assert PurePosixPath('src/main.py').full_match('src/**/*.py')
assert p.full_match('SRC/**/MAIN.PY', case_sensitive=False)
assert not p.full_match('SRC/**/MAIN.PY', case_sensitive=True)
