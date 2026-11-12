import csv
import io
import json

text = await read_file({"path": "/data/users.jsonl"})
users = [json.loads(line) for line in text.splitlines() if line.strip()]
out = io.StringIO(newline="")
writer = csv.writer(out, lineterminator="\n")
writer.writerow(["id", "username", "email", "roles", "active", "last_login"])
for u in sorted(users, key=lambda u: u["id"]):
    writer.writerow([u["id"], u["username"], u["email"], ";".join(u["roles"]), "true" if u["active"] else "false", u.get("last_login") or ""])
await write_file({"path": "/out/users.csv", "content": out.getvalue()})
