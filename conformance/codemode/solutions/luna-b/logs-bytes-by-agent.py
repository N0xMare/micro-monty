import re

text = await get_access_log({"date": "2026-03-14"})
totals = {"bot": 0, "script": 0, "browser": 0}
pattern = re.compile(r'^\S+\s+\S+\s+\S+\s+\[[^]]+\]\s+"[^"]*"\s+\d{3}\s+(\d+|-)\s+"[^"]*"\s+"([^"]*)"')
for line in text.splitlines():
    match = pattern.search(line)
    if not match:
        continue
    size = 0 if match.group(1) == "-" else int(match.group(1))
    agent = match.group(2)
    if "bot" in agent.lower():
        category = "bot"
    elif agent.lower().startswith(("curl", "python-requests")):
        category = "script"
    else:
        category = "browser"
    totals[category] += size
await submit_answer({"bytes": totals})
