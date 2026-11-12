import json
from collections import Counter

text = await read_file({"path": "/data/users.jsonl"})
counts = Counter()
for line in text.splitlines():
    if line.strip():
        user = json.loads(line)
        if user["active"]:
            counts.update(set(user["roles"]))
await submit_answer({"roles": dict(counts)})
