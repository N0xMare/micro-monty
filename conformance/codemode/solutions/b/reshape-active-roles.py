import json

text = await read_file({"path": "/data/users.jsonl"})
users = [json.loads(line) for line in text.splitlines() if line.strip()]

def is_active(v):
    if isinstance(v, str):
        return v.strip().lower() in ("true", "1", "yes", "y")
    return bool(v)

roles = {}
for u in users:
    if not is_active(u.get("active")):
        continue
    r = u.get("roles") or []
    if isinstance(r, str):
        r = [x.strip() for x in r.replace(";", ",").split(",") if x.strip()]
    for role in dict.fromkeys(r):
        roles[role] = roles.get(role, 0) + 1

print(len(users), roles)
print(await submit_answer({"roles": roles}))
