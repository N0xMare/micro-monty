import re

text = await fetch_logs({"service": "gateway", "date": "2026-03-14"})
line_re = re.compile(r'^(\S+)\s+(DEBUG|INFO|WARN|ERROR)\s')
lines = []
for line in text.splitlines():
    line = line.rstrip("\r")
    m = line_re.match(line)
    if not m or m.group(2) not in ("WARN", "ERROR"):
        continue
    ts = m.group(1)
    if ts.startswith("2026-03-14T") and "12:00:00" <= ts[11:19] < "13:00:00":
        lines.append(line)
print(len(lines), "lines")
print(*lines, sep="\n")
print(await submit_answer({"lines": lines}))
