import csv
import io
import json

text = await read_file({"path": "/data/employees.csv"})
employees = [{"id": int(r["id"]), "name": r["name"], "title": r["title"], "salary": int(r["salary"])} for r in csv.DictReader(io.StringIO(text)) if r["department"] == "Engineering"]
employees.sort(key=lambda r: (-r["salary"], r["id"]))
await write_file({"path": "/out/engineering.json", "content": json.dumps(employees)})
