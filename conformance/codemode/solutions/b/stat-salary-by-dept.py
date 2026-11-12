import csv, io, statistics
from collections import defaultdict

text = await read_file({"path": "/data/employees.csv"})
reader = csv.DictReader(io.StringIO(text.lstrip("\ufeff")))
cols = {c.strip().lower(): c for c in reader.fieldnames}
print("columns:", reader.fieldnames)
dept_col = next(cols[k] for k in cols if k in ("department", "dept", "team"))
sal_col = next(cols[k] for k in cols if "salary" in k)

by_dept = defaultdict(list)
for row in reader:
    raw = (row.get(sal_col) or "").strip().replace("$", "").replace(",", "")
    dept = (row.get(dept_col) or "").strip()
    if not raw or not dept:
        continue
    try:
        by_dept[dept].append(float(raw))
    except ValueError:
        print("skipping row:", row)

def num(x):
    return int(x) if float(x).is_integer() else x

departments = {
    d: {"median": num(statistics.median(v)), "mean": round(statistics.mean(v), 2)}
    for d, v in sorted(by_dept.items())
}
print(departments)
print(await submit_answer({"departments": departments}))
