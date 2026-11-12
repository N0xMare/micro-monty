import csv, io, statistics
from collections import defaultdict

text = await read_file({"path": "/data/employees.csv"})
reader = csv.DictReader(io.StringIO(text))
cols = {c.strip().lower(): c for c in reader.fieldnames}
dept_col = cols.get("department") or cols.get("dept")
sal_col = cols.get("salary")

by_dept = defaultdict(list)
for row in reader:
    dept = (row.get(dept_col) or "").strip()
    sal = (row.get(sal_col) or "").strip().replace(",", "").replace("$", "")
    if not dept or not sal:
        continue
    try:
        by_dept[dept].append(float(sal))
    except ValueError:
        continue

def num(x):
    return int(x) if float(x).is_integer() else x

result = {
    d: {"median": num(statistics.median(v)), "mean": round(statistics.mean(v), 2)}
    for d, v in sorted(by_dept.items())
}
await submit_answer({"departments": result})
