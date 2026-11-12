import re

text = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
matches = []
for line in text.splitlines():
    ts = re.match(r"^(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)", line)
    if ts and "T12:00:00" <= ts.group(1)[10:] < "T13:00:00" and re.search(r"\s(?:WARN|ERROR)\s", line):
        matches.append(line)
await submit_answer({"lines": matches})
