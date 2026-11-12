import csv
import io
from collections import defaultdict

text = await read_file({"path": "/data/employees.csv"})
departments = defaultdict(list)
for row in csv.DictReader(io.StringIO(text)):
    departments[row["department"]].append(row["name"])
await submit_answer({"departments": {d: sorted(names) for d, names in departments.items()}})
