import csv, io

text = (await read_file({"path": "/data/employees.csv"})).lstrip("\ufeff")
reader = csv.DictReader(io.StringIO(text))
print("columns:", reader.fieldnames)
fields = {f.strip().lower(): f for f in reader.fieldnames}
name_col = fields.get("name") or next(f for k, f in fields.items() if "name" in k)
dept_col = fields.get("department") or next(f for k, f in fields.items() if "dept" in k)

departments = {}
for row in reader:
    name = (row[name_col] or "").strip()
    dept = (row[dept_col] or "").strip()
    if name:
        departments.setdefault(dept, []).append(name)
departments = {d: sorted(n) for d, n in sorted(departments.items())}

print(departments)
print(await submit_answer({"departments": departments}))
