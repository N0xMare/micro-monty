import json

users = [json.loads(line) for line in (await read_file({"path": "/data/users.jsonl"})).splitlines() if line.strip()]
usernames = sorted(user["username"] for user in users if "last_login" not in user)
await submit_answer({"usernames": usernames})
