import re

text = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
lines = []
for line in text.splitlines():
    match = re.match(r"\d{4}-\d\d-\d\dT(\d\d):(\d\d):(\d\d)\.\d+Z\s+(\w+)", line)
    if match and match.group(4) in {"WARN", "ERROR"} and (match.group(1), match.group(2)) >= ("12", "00") and (match.group(1), match.group(2)) < ("13", "00"):
        lines.append(line)
await submit_answer({"lines": lines})
