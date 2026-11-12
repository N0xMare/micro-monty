from collections import Counter

text = await get_access_log({"date": "2026-03-14"})
totals = Counter({"bot": 0, "script": 0, "browser": 0})
for line in text.splitlines():
    fields = line.split('"')
    if len(fields) < 6:
        continue
    pre = fields[0].split()
    if len(pre) < 1:
        continue
    status_bytes = fields[2].split()
    if len(status_bytes) < 2:
        continue
    try:
        size = int(status_bytes[1])
    except ValueError:
        size = 0
    agent = fields[5].strip().lower()
    kind = "bot" if "bot" in agent else "script" if agent.startswith(("curl", "python-requests")) else "browser"
    totals[kind] += size
await submit_answer({"bytes": dict(totals)})
