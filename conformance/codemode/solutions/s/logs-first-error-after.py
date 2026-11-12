import re

text = await fetch_logs({"service": "billing", "date": "2026-03-15"})
line_re = re.compile(r'^(\S+)\s+(DEBUG|INFO|WARN|ERROR)\s+billing\[\d+\]: (.*)$')
found = None
for line in text.splitlines():
    m = line_re.match(line.rstrip("\r"))
    if not m or m.group(2) != "ERROR":
        continue
    ts = m.group(1)
    if not ts.startswith("2026-03-15T"):
        continue
    if ts[11:19] >= "06:00:00":
        if found is None or ts < found[0]:
            found = (ts, m.group(3))
print(found)
if found:
    print(await submit_answer({"ts": found[0], "message": found[1]}))
