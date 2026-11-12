import csv, io

text = await read_file({"path": "/data/employees.csv"})
rows = list(csv.DictReader(io.StringIO(text)))
rows = [{(k or "").strip(): (v or "").strip() for k, v in r.items()} for r in rows]

names = {int(r["id"]): r["name"] for r in rows if r.get("id")}
reports = []
for r in rows:
    if not r.get("id"):
        continue
    mid = r.get("manager_id", "")
    if not mid:
        continue
    mid = int(float(mid))
    reports.append({"id": int(r["id"]), "name": r["name"], "manager": names.get(mid)})

reports.sort(key=lambda x: x["id"])
missing = [r for r in reports if r["manager"] is None]
print(len(reports), "reports; unresolved managers:", missing)
print(await submit_answer({"reports": reports}))
