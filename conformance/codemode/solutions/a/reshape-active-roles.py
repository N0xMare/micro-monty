import json
from collections import Counter

def truthy(v):
    if isinstance(v, str):
        return v.strip().lower() in ("true", "1", "yes", "y")
    return bool(v)

text = await read_file({"path": "/data/users.jsonl"})
roles = Counter()
for line in text.splitlines():
    line = line.strip()
    if not line:
        continue
    u = json.loads(line)
    if not truthy(u.get("active")):
        continue
    r = u.get("roles") or []
    if isinstance(r, str):
        r = [x.strip() for x in r.replace(";", ",").split(",") if x.strip()]
    for role in set(r):
        roles[role] += 1
roles = dict(sorted(roles.items()))
print(roles)
print(await submit_answer({"roles": roles}))
