import csv
import io
import json
text = await read_file({"path": "/data/users.jsonl"})
users = [json.loads(line) for line in text.splitlines() if line.strip()]
users.sort(key=lambda x: x["id"])
out = io.StringIO(newline="")
writer = csv.writer(out, lineterminator="\n")
writer.writerow(["id", "username", "email", "roles", "active", "last_login"])
for user in users:
    writer.writerow([user["id"], user["username"], user["email"], ";".join(user.get("roles", [])), str(bool(user["active"])).lower(), user.get("last_login", "")])
await write_file({"path": "/out/users.csv", "content": out.getvalue()})
