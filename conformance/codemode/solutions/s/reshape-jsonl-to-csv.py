import csv, io, json

text = await read_file({"path": "/data/users.jsonl"})
users = [json.loads(line) for line in text.splitlines() if line.strip()]
print(users[:3])

users.sort(key=lambda u: int(u["id"]))

buf = io.StringIO()
writer = csv.writer(buf, lineterminator="\n")
writer.writerow(["id", "username", "email", "roles", "active", "last_login"])
for u in users:
    roles = u.get("roles") or []
    if isinstance(roles, str):
        roles = [roles]
    last_login = u.get("last_login")
    writer.writerow([
        u["id"],
        u.get("username", ""),
        u.get("email", ""),
        ";".join(roles),
        "true" if u.get("active") else "false",
        "" if last_login is None else last_login,
    ])
content = buf.getvalue()
print(content)

res = await write_file({"path": "/out/users.csv", "content": content})
print(res)
