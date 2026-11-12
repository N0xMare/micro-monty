import csv, io, json

text = await read_file({"path": "/data/users.jsonl"})
users = [json.loads(line) for line in text.splitlines() if line.strip()]
users.sort(key=lambda u: int(u["id"]))

def active(v):
    if isinstance(v, str):
        v = v.strip().lower() in ("true", "1", "yes", "y")
    return "true" if v else "false"

buf = io.StringIO()
writer = csv.writer(buf, lineterminator="\n")
writer.writerow(["id", "username", "email", "roles", "active", "last_login"])
for u in users:
    roles = u.get("roles") or []
    if isinstance(roles, str):
        roles = [roles]
    last = u.get("last_login")
    writer.writerow([u["id"], u.get("username", ""), u.get("email", ""), ";".join(roles),
                     active(u.get("active")), "" if last is None else last])
content = buf.getvalue()

print(content)
print(await write_file({"path": "/out/users.csv", "content": content}))

# submit_answer is not in this prompt's tool catalog; call it only if the harness injected it.
if "submit_answer" in globals():
    print(await submit_answer({"path": "/out/users.csv", "rows": len(users)}))
