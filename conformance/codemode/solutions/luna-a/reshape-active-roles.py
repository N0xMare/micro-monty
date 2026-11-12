import json
from collections import Counter

users = [json.loads(line) for line in (await read_file({"path": "/data/users.jsonl"})).splitlines() if line.strip()]
counts = Counter(role for user in users if user.get("active") for role in user.get("roles", []))
await submit_answer({"roles": dict(counts)})
