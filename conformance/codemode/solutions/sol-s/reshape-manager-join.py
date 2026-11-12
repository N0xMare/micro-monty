import csv
import io

text = await read_file({"path": "/data/employees.csv"})
employees = list(csv.DictReader(io.StringIO(text)))
names = {int(e["id"]): e["name"] for e in employees}
reports = [{"id": int(e["id"]), "name": e["name"], "manager": names[int(e["manager_id"])]} for e in employees if e["manager_id"].strip()]
await submit_answer({"reports": sorted(reports, key=lambda e: e["id"])})
