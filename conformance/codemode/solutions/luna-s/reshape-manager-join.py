import csv
import io
text = await read_file({"path": "/data/employees.csv"})
employees = list(csv.DictReader(io.StringIO(text)))
names = {row["id"]: row["name"] for row in employees}
reports = [{"id": int(row["id"]), "name": row["name"], "manager": names[row["manager_id"]]} for row in employees if row.get("manager_id")]
reports.sort(key=lambda row: row["id"])
await submit_answer({"reports": reports})
