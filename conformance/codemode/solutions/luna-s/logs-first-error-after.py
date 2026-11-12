import re

text = await fetch_logs({"service": "billing", "date": "2026-03-15"})
for line in text.splitlines():
    match = re.match(r"^(\S+)\s+ERROR\s+billing\[\d+\]:\s?(.*)$", line)
    if match and match.group(1)[11:19] >= "06:00:00":
        await submit_answer({"ts": match.group(1), "message": match.group(2)})
        break
