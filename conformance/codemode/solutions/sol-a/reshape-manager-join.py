import csv
import io

rows = list(csv.DictReader(io.StringIO(await read_file({"path": "/data/employees.csv"}))))
names = {int(r["id"]): r["name"] for r in rows}
reports = [{"id": int(r["id"]), "name": r["name"], "manager": names[int(r["manager_id"])]} for r in rows if r["manager_id"].strip()]
await submit_answer({"reports": sorted(reports, key=lambda r: r["id"])})
