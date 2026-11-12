import json
text = await read_file({"path": "/data/users.jsonl"})
users = [json.loads(line) for line in text.splitlines() if line.strip()]
await submit_answer({"usernames": sorted(user["username"] for user in users if "last_login" not in user)})
