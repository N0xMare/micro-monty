import json
text = await read_file({"path": "/data/users.jsonl"})
counts = {}
for line in text.splitlines():
    if not line.strip():
        continue
    user = json.loads(line)
    if user.get("active"):
        for role in user.get("roles", []):
            counts[role] = counts.get(role, 0) + 1
await submit_answer({"roles": counts})
