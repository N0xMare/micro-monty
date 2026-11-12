import re

log = await get_access_log({"date": "2026-03-14"})
totals = dict.fromkeys(["bot", "script", "browser"], 0)
pattern = re.compile(r'^\S+ \S+ \S+ \[[^\]]+\] "(?:[^"\\]|\\.)*" \d{3} (\d+|-) "(?:[^"\\]|\\.)*" "((?:[^"\\]|\\.)*)"$')
for line in log.splitlines():
    match = pattern.match(line)
    if not match:
        continue
    size, agent = match.groups()
    category = "bot" if "bot" in agent.lower() else "script" if agent.startswith(("curl", "python-requests")) else "browser"
    totals[category] += 0 if size == "-" else int(size)
await submit_answer({"bytes": totals})
