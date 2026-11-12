import csv
import io
import json
import statistics

text = await read_file({"path": "/data/employees.csv"})
groups = {}
for row in csv.DictReader(io.StringIO(text)):
    groups.setdefault(row["department"], []).append(float(row["salary"].replace(",", "").replace("$", "")))
departments = {dept: {"median": statistics.median(vals), "mean": round(statistics.mean(vals), 2)} for dept, vals in groups.items()}
await submit_answer({"departments": departments})
