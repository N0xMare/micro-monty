import csv
import io
import json

rows = list(csv.DictReader(io.StringIO(await read_file({"path": "/data/employees.csv"}))))
engineering = [
    {"id": int(row["id"]), "name": row["name"], "title": row["title"], "salary": int(row["salary"])}
    for row in rows if row["department"] == "Engineering"
]
engineering.sort(key=lambda employee: (-employee["salary"], employee["id"]))
await write_file({"path": "/out/engineering.json", "content": json.dumps(engineering, indent=2) + "\n"})
