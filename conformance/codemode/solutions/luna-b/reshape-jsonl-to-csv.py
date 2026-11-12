import csv, io, json

text = await read_file({"path": "/data/users.jsonl"})
users = [json.loads(line) for line in text.splitlines()]
users.sort(key=lambda user: user["id"])
buffer = io.StringIO(newline="")
writer = csv.writer(buffer, lineterminator="\n")
header = ["id", "username", "email", "roles", "active", "last_login"]
writer.writerow(header)
for user in users:
    writer.writerow([user["id"], user["username"], user["email"], ";".join(user.get("roles", [])),
                     str(user["active"]).lower(), user.get("last_login", "")])
await write_file({"path": "/out/users.csv", "content": buffer.getvalue()})
