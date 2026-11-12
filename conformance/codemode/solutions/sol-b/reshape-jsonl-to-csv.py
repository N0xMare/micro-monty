import json, csv, io

users = [json.loads(line) for line in (await read_file({"path": "/data/users.jsonl"})).splitlines() if line.strip()]
out = io.StringIO(newline="")
writer = csv.writer(out, lineterminator="\n")
writer.writerow(["id", "username", "email", "roles", "active", "last_login"])
for u in sorted(users, key=lambda u: u["id"]):
    writer.writerow([u["id"], u["username"], u["email"], ";".join(u["roles"]), str(u["active"]).lower(), u.get("last_login", "")])
await write_file({"path": "/out/users.csv", "content": out.getvalue()})
