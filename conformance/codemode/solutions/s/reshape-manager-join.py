import csv, io

text = await read_file({"path": "/data/employees.csv"})
rows = list(csv.DictReader(io.StringIO(text.lstrip("\ufeff"))))
rows = [{(k or "").strip(): (v or "").strip() for k, v in r.items()} for r in rows]

names = {}
for r in rows:
    if r.get("id"):
        names[int(r["id"])] = r["name"]

reports = []
missing = []
for r in rows:
    if not r.get("id"):
        continue
    mid = r.get("manager_id", "")
    if not mid:
        continue
    mid = int(float(mid))
    if mid not in names:
        missing.append((r["id"], mid))
        continue
    reports.append({"id": int(r["id"]), "name": r["name"], "manager": names[mid]})

reports.sort(key=lambda x: x["id"])
if missing:
    print("manager ids not found:", missing)
print(reports)
print(await submit_answer({"reports": reports}))
