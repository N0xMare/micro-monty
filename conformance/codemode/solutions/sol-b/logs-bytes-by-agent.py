import re

log = await get_access_log({"date": "2026-03-14"})
totals = {"bot": 0, "script": 0, "browser": 0}
pattern = re.compile(r'^\S+ .*?\[.*?\] "(?:[^"\\]|\\.)*" \d{3} (\d+|-) "(?:[^"\\]|\\.)*" "((?:[^"\\]|\\.)*)"$')
for line in log.splitlines():
    match = pattern.match(line)
    if match:
        size, agent = match.groups()
        lower = agent.lower()
        kind = "bot" if "bot" in lower else "script" if agent.startswith(("curl", "python-requests")) else "browser"
        totals[kind] += 0 if size == "-" else int(size)
await submit_answer({"bytes": totals})
