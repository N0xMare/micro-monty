import csv, io, json

text = (await read_file({"path": "/data/employees.csv"})).lstrip("\ufeff")
rows = list(csv.DictReader(io.StringIO(text)))
rows = [{k.strip().lower(): (v or "").strip() for k, v in r.items()} for r in rows]
print(rows[:2])
dept_key = "department" if "department" in rows[0] else "dept"

def to_int(s):
    return int(round(float(s.replace(",", "").replace("$", ""))))

eng = [
    {"id": int(r["id"]), "name": r["name"], "title": r["title"], "salary": to_int(r["salary"])}
    for r in rows if r[dept_key].lower() == "engineering"
]
eng.sort(key=lambda e: (-e["salary"], e["id"]))
content = json.dumps(eng, indent=2) + "\n"
print(content)
print(await write_file({"path": "/out/engineering.json", "content": content}))
