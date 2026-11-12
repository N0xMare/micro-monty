import json
from collections import Counter

text = await read_file({"path": "/data/users.jsonl"})
roles = Counter()
for line in text.splitlines():
    if line.strip():
        user = json.loads(line)
        if user["active"]:
            roles.update(set(user["roles"]))
await submit_answer({"roles": dict(roles)})
