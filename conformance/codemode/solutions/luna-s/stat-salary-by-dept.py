import csv
import statistics
from io import StringIO

text = await read_file({"path": "/data/employees.csv"})
groups = {}
for row in csv.DictReader(StringIO(text)):
    groups.setdefault(row["department"], []).append(float(row["salary"]))
departments = {name: {"median": statistics.median(salaries), "mean": round(statistics.mean(salaries), 2)} for name, salaries in sorted(groups.items())}
await submit_answer({"departments": departments})
