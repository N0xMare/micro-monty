import csv, io, json

text = await read_file({"path": "/data/users.jsonl"})
users = [json.loads(l) for l in text.splitlines() if l.strip()]

def truthy(v):
    if isinstance(v, str):
        return v.strip().lower() in ("true", "1", "yes", "y")
    return bool(v)

users.sort(key=lambda u: int(u["id"]))
buf = io.StringIO()
wr = csv.writer(buf, lineterminator="\n")
wr.writerow(["id", "username", "email", "roles", "active", "last_login"])
for u in users:
    roles = u.get("roles") or []
    if isinstance(roles, str):
        roles = [roles]
    last = u.get("last_login")
    wr.writerow([u["id"], u["username"], u["email"], ";".join(roles),
                 "true" if truthy(u.get("active")) else "false",
                 "" if last is None else last])
content = buf.getvalue()
print(content)
print(await write_file({"path": "/out/users.csv", "content": content}))
