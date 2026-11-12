import csv
import io
import json
import statistics

text = await read_file({"path": "/data/employees.csv"})
rows = csv.DictReader(io.StringIO(text))
groups = {}
for row in rows:
    groups.setdefault(row["department"], []).append(float(row["salary"]))
departments = {name: {"median": statistics.median(vals), "mean": round(statistics.mean(vals), 2)} for name, vals in sorted(groups.items())}
await submit_answer({"departments": departments})
