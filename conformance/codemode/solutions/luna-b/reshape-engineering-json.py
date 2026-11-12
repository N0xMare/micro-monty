import csv, io, json

text = await read_file({"path": "/data/employees.csv"})
employees = []
for row in csv.DictReader(io.StringIO(text)):
    if row["department"] == "Engineering":
        employees.append({"id": int(row["id"]), "name": row["name"], "title": row["title"], "salary": int(row["salary"])})
employees.sort(key=lambda employee: (-employee["salary"], employee["id"]))
await write_file({"path": "/out/engineering.json", "content": json.dumps(employees, indent=2) + "\n"})
