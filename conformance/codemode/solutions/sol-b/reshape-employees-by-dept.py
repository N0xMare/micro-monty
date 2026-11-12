import csv, io
from collections import defaultdict

rows = csv.DictReader(io.StringIO(await read_file({"path": "/data/employees.csv"})))
departments = defaultdict(list)
for row in rows:
    departments[row["department"]].append(row["name"])
await submit_answer({"departments": {k: sorted(v) for k, v in departments.items()}})
