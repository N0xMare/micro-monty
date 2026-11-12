import csv, io, json, re

text = await read_file({"path": "/data/employees.csv"})
rows = list(csv.DictReader(io.StringIO(text.lstrip("﻿"))))
rows = [{k.strip().lower(): (v or "").strip() for k, v in r.items()} for r in rows]
print(rows[:3])

def to_int(s):
    s = re.sub(r"[^\d.\-]", "", s)
    return int(round(float(s)))

dept_key = "department" if "department" in rows[0] else "dept"
eng = [
    {"id": to_int(r["id"]), "name": r["name"], "title": r["title"], "salary": to_int(r["salary"])}
    for r in rows
    if r[dept_key].lower() == "engineering"
]
eng.sort(key=lambda e: (-e["salary"], e["id"]))

content = json.dumps(eng, indent=2, ensure_ascii=False) + "\n"
print(content)
res = await write_file({"path": "/out/engineering.json", "content": content})
print(res)
