import csv, io
from collections import defaultdict

text = (await read_file({"path": "/data/employees.csv"})).lstrip("\ufeff")
rows = list(csv.DictReader(io.StringIO(text)))
print(rows[:2])
keys = {k.strip().lower(): k for k in rows[0]}
dept_key = keys.get("department") or keys.get("dept")
name_key = keys.get("name") or keys.get("full_name")

departments = defaultdict(list)
for r in rows:
    departments[r[dept_key].strip()].append(r[name_key].strip())
departments = {d: sorted(n) for d, n in sorted(departments.items())}
print(departments)
print(await submit_answer({"departments": departments}))
