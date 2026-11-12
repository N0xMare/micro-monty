import csv
import io
import json

rows = csv.DictReader(io.StringIO(await read_file({"path": "/data/employees.csv"})))
employees = [{"id": int(r["id"]), "name": r["name"], "title": r["title"], "salary": int(r["salary"])} for r in rows if r["department"] == "Engineering"]
employees.sort(key=lambda e: (-e["salary"], e["id"]))
await write_file({"path": "/out/engineering.json", "content": json.dumps(employees)})
