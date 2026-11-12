import re

text = await get_access_log({"date": "2026-03-14"})
totals = {"bot": 0, "script": 0, "browser": 0}
pattern = re.compile(r'\]\s+"[^"]*"\s+\d{3}\s+(\d+)\s+"[^"]*"\s+"([^"]*)"\s*$')
for line in text.splitlines():
    match = pattern.search(line)
    if not match:
        continue
    size, agent = int(match.group(1)), match.group(2)
    lower = agent.lower()
    category = "bot" if "bot" in lower else "script" if lower.startswith(("curl", "python-requests")) else "browser"
    totals[category] += size
await submit_answer({"bytes": totals})
