import csv, io, json

text = (await read_file({"path": "/data/employees.csv"})).lstrip("\ufeff")
reader = csv.DictReader(io.StringIO(text))
print("columns:", reader.fieldnames)

def get(row, key):
    for k, v in row.items():
        if k and k.strip().lower() == key:
            return (v or "").strip()
    raise KeyError(key)

eng = []
for row in reader:
    if get(row, "department") != "Engineering":
        continue
    eng.append({
        "id": int(get(row, "id")),
        "name": get(row, "name"),
        "title": get(row, "title"),
        "salary": int(float(get(row, "salary").replace(",", "").replace("$", ""))),
    })
eng.sort(key=lambda e: (-e["salary"], e["id"]))
content = json.dumps(eng, indent=2) + "\n"

print(content)
print(await write_file({"path": "/out/engineering.json", "content": content}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"path": "/out/engineering.json", "count": len(eng)}))
