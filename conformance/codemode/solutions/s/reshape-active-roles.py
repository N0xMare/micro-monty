import json
from collections import Counter

text = await read_file({"path": "/data/users.jsonl"})
users = [json.loads(line) for line in text.splitlines() if line.strip()]
print(users[:3])

def is_active(u):
    if "active" in u:
        v = u["active"]
        if isinstance(v, str):
            return v.strip().lower() in ("true", "yes", "1", "y")
        return bool(v)
    if "status" in u:
        return str(u["status"]).strip().lower() == "active"
    return False

def roles_of(u):
    r = u.get("roles", u.get("role", []))
    if r is None:
        return []
    if isinstance(r, str):
        r = [x.strip() for x in r.split(",")]
    return [x for x in dict.fromkeys(r) if x]

roles = Counter()
for u in users:
    if is_active(u):
        roles.update(roles_of(u))

roles = dict(sorted(roles.items()))
print(roles)
await submit_answer({"roles": roles})
