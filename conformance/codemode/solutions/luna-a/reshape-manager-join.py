import csv
import io

rows = list(csv.DictReader(io.StringIO(await read_file({"path": "/data/employees.csv"}))))
names = {row["id"]: row["name"] for row in rows}
reports = [
    {"id": int(row["id"]), "name": row["name"], "manager": names[row["manager_id"]]}
    for row in rows if row.get("manager_id", "").strip()
]
reports.sort(key=lambda report: report["id"])
await submit_answer({"reports": reports})
