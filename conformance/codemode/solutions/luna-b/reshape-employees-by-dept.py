import csv, io
from collections import defaultdict

text = await read_file({"path": "/data/employees.csv"})
departments = defaultdict(list)
for row in csv.DictReader(io.StringIO(text)):
    departments[row["department"]].append(row["name"])
await submit_answer({"departments": {department: sorted(names) for department, names in departments.items()}})
