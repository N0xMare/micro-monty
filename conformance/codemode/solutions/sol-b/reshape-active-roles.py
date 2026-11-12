import json
from collections import Counter

users = [json.loads(line) for line in (await read_file({"path": "/data/users.jsonl"})).splitlines() if line.strip()]
roles = Counter(role for u in users if u["active"] for role in set(u["roles"]))
await submit_answer({"roles": dict(roles)})
