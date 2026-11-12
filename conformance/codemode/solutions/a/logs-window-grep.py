import re
from datetime import datetime, timezone

text = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
line_re = re.compile(r'^(\S+)\s+(WARN|ERROR)\s')
start = datetime(2026, 3, 14, 12, 0, tzinfo=timezone.utc)
end = datetime(2026, 3, 14, 13, 0, tzinfo=timezone.utc)
lines = []
for line in text.splitlines():
    line = line.rstrip("\r")
    m = line_re.match(line)
    if not m:
        continue
    t = datetime.fromisoformat(m.group(1).replace("Z", "+00:00"))
    if start <= t < end:
        lines.append(line)
print(len(lines), "lines")
print("\n".join(lines))
print(await submit_answer({"lines": lines}))
