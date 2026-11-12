import csv
import io
text = await read_file({"path": "/data/employees.csv"})
departments = {}
for row in csv.DictReader(io.StringIO(text)):
    departments.setdefault(row["department"], []).append(row["name"])
for names in departments.values():
    names.sort()
await submit_answer({"departments": departments})
