import json

users = [json.loads(line) for line in (await read_file({"path": "/data/users.jsonl"})).splitlines() if line.strip()]
await submit_answer({"usernames": sorted(u["username"] for u in users if "last_login" not in u)})
