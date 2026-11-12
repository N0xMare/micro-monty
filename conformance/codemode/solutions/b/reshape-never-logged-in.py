import json

text = await read_file({"path": "/data/users.jsonl"})
usernames = []
bad = 0
for line in text.splitlines():
    line = line.strip()
    if not line:
        continue
    try:
        user = json.loads(line)
    except json.JSONDecodeError:
        bad += 1
        continue
    if "last_login" not in user:
        usernames.append(user["username"])

usernames.sort()
if bad:
    print("unparseable lines:", bad)
print(usernames)
print(await submit_answer({"usernames": usernames}))
