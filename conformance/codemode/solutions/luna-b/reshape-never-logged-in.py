import json

text = await read_file({"path": "/data/users.jsonl"})
usernames = sorted(user["username"] for user in map(json.loads, text.splitlines()) if "last_login" not in user)
await submit_answer({"usernames": usernames})
