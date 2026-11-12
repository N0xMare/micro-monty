import asyncio, json, re
from collections import Counter

services = json.loads(await list_services({}))
texts = await asyncio.gather(*(fetch_logs({"service": s, "date": "2026-03-14"}) for s in services))

line_re = re.compile(r'^(\S+)\s+(\w+)\s+([\w.-]+)\[(\d+)\]:\s(.*)$')
kv_re = re.compile(r'(\w+)=(\S+)')
users = Counter()
heads = Counter()
for text in texts:
    for line in text.splitlines():
        m = line_re.match(line)
        if not m:
            continue
        msg = m.group(5)
        kv = dict(kv_re.findall(msg))
        if "user" in kv and re.fullmatch(r"\d{3}", kv.get("status", "")):
            users[kv["user"]] += 1
            heads[msg.split(" ", 2)[:2].__str__()] += 1
print("completed-request message heads:", heads.most_common(5))
ranked = sorted(users.items(), key=lambda kv: (-kv[1], kv[0]))
print(ranked[:5])
user, n = ranked[0]
print(await submit_answer({"user": user, "requests": n}))
