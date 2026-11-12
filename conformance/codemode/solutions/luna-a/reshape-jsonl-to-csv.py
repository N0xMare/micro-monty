import csv
import io
import json

users = [json.loads(line) for line in (await read_file({"path": "/data/users.jsonl"})).splitlines() if line.strip()]
users.sort(key=lambda user: user["id"])
output = io.StringIO(newline="")
writer = csv.writer(output, lineterminator="\n")
writer.writerow(["id", "username", "email", "roles", "active", "last_login"])
for user in users:
    writer.writerow([
        user["id"], user["username"], user["email"], ";".join(user.get("roles", [])),
        str(bool(user["active"])).lower(), user.get("last_login", "") or "",
    ])
await write_file({"path": "/out/users.csv", "content": output.getvalue()})
