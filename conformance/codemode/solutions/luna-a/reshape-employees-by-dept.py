import csv
import io
from collections import defaultdict

rows = csv.DictReader(io.StringIO(await read_file({"path": "/data/employees.csv"})))
departments = defaultdict(list)
for row in rows:
    departments[row["department"]].append(row["name"])
await submit_answer({"departments": {department: sorted(names) for department, names in departments.items()}})
