import csv, io, statistics
from collections import defaultdict

text = await read_file({"path": "/data/employees.csv"})
reader = csv.DictReader(io.StringIO(text))
fields = {f.strip().lower(): f for f in reader.fieldnames}
dept_col = fields.get("department") or fields.get("dept")
sal_col = fields.get("salary")
print(reader.fieldnames)

groups = defaultdict(list)
for row in reader:
    dept = (row.get(dept_col) or "").strip()
    raw = (row.get(sal_col) or "").strip().replace(",", "").replace("$", "")
    if not dept or not raw:
        continue
    try:
        groups[dept].append(float(raw))
    except ValueError:
        continue

departments = {}
for dept, sals in sorted(groups.items()):
    med = statistics.median(sals)
    departments[dept] = {
        "median": int(med) if med == int(med) else round(med, 2),
        "mean": round(statistics.mean(sals), 2),
    }
print(departments)
await submit_answer({"departments": departments})
