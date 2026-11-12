import json
from collections import Counter

text = await read_file({"path": "/data/users.jsonl"})
counts = Counter()
for line in text.splitlines():
    user = json.loads(line)
    if user["active"]:
        counts.update(user.get("roles", []))
await submit_answer({"roles": dict(counts)})
