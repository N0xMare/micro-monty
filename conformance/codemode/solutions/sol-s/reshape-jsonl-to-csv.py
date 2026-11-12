import csv
import io
import json

text = await read_file({"path": "/data/users.jsonl"})
users = sorted((json.loads(line) for line in text.splitlines() if line.strip()), key=lambda u: u["id"])
output = io.StringIO(newline="")
writer = csv.writer(output, lineterminator="\n")
writer.writerow(["id", "username", "email", "roles", "active", "last_login"])
for user in users:
    writer.writerow([user["id"], user["username"], user["email"], ";".join(user["roles"]), str(user["active"]).lower(), user.get("last_login") or ""])
await write_file({"path": "/out/users.csv", "content": output.getvalue()})
