import csv, io

text = await read_file({"path": "/data/employees.csv"})
employees = list(csv.DictReader(io.StringIO(text)))
names = {row["id"]: row["name"] for row in employees}
reports = [{"id": int(row["id"]), "name": row["name"], "manager": names[row["manager_id"]]}
           for row in employees if row["manager_id"]]
reports.sort(key=lambda report: report["id"])
await submit_answer({"reports": reports})
