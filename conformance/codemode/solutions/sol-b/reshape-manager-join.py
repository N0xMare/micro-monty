import csv, io

employees = list(csv.DictReader(io.StringIO(await read_file({"path": "/data/employees.csv"}))))
names = {int(e["id"]): e["name"] for e in employees}
reports = [{"id": int(e["id"]), "name": e["name"], "manager": names[int(e["manager_id"])]} for e in employees if e["manager_id"].strip()]
await submit_answer({"reports": sorted(reports, key=lambda r: r["id"])})
