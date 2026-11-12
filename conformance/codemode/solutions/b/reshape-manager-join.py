import csv
import io

text = await read_file({"path": "/data/employees.csv"})
rows = list(csv.DictReader(io.StringIO(text)))
rows = [{(k or "").strip(): (v or "").strip() for k, v in r.items()} for r in rows]
by_id = {int(r["id"]): r["name"] for r in rows if r.get("id")}

reports = []
missing = []
for r in rows:
    if not r.get("id"):
        continue
    mid = r.get("manager_id", "")
    if not mid:
        continue
    mid = int(float(mid))
    if mid not in by_id:
        missing.append((r["id"], mid))
        continue
    reports.append({"id": int(r["id"]), "name": r["name"], "manager": by_id[mid]})

reports.sort(key=lambda x: x["id"])
if missing:
    print("manager ids not found:", missing)
print(len(reports), reports[:5])
print(await submit_answer({"reports": reports}))
