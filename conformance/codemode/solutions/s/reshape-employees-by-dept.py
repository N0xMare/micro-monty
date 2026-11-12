import csv, io
from collections import defaultdict

text = await read_file({"path": "/data/employees.csv"})
rows = list(csv.DictReader(io.StringIO(text.lstrip("\ufeff"))))
print(rows[:3])

def find(keys, *cands):
    for k in keys:
        if k.strip().lower() in cands:
            return k
    for k in keys:
        if any(c in k.strip().lower() for c in cands):
            return k
    raise KeyError(cands)

keys = list(rows[0].keys())
dept_key = find(keys, "department", "dept")
if any(k.strip().lower() == "name" for k in keys):
    name_of = lambda r: r[find(keys, "name")].strip()
else:
    fk, lk = find(keys, "first_name", "firstname", "first"), find(keys, "last_name", "lastname", "last")
    name_of = lambda r: f"{r[fk].strip()} {r[lk].strip()}"

departments = defaultdict(list)
for r in rows:
    d = (r[dept_key] or "").strip()
    if d:
        departments[d].append(name_of(r))

departments = {d: sorted(names) for d, names in sorted(departments.items())}
print(departments)
await submit_answer({"departments": departments})
