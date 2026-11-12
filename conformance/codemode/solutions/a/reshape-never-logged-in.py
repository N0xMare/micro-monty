import json

text = await read_file({"path": "/data/users.jsonl"})
names = []
bad = 0
for line in text.splitlines():
    line = line.strip()
    if not line:
        continue
    try:
        u = json.loads(line)
    except json.JSONDecodeError:
        bad += 1
        continue
    if "last_login" not in u:
        names.append(u["username"])

names.sort()
print(len(names), names, "unparseable lines:", bad)
print(await submit_answer({"usernames": names}))
