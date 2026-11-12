import re

text = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
lines = []
for line in text.splitlines():
    match = re.match(r"^(\S+)\s+(WARN|ERROR)\s+", line)
    if match and "12:00:00" <= match.group(1)[11:19] < "13:00:00":
        lines.append(line)
await submit_answer({"lines": lines})
