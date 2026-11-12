import json

text = await read_file({"path": "/data/users.jsonl"})
usernames = []
bad = []
for i, line in enumerate(text.splitlines(), 1):
    line = line.strip()
    if not line:
        continue
    try:
        u = json.loads(line)
    except json.JSONDecodeError:
        bad.append(i)
        continue
    if "last_login" not in u:
        usernames.append(u["username"])

usernames.sort()
if bad:
    print("unparseable lines:", bad)
print(usernames)
print(await submit_answer({"usernames": usernames}))
