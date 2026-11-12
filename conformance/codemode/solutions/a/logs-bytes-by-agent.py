import re

log = await get_access_log({"date": "2026-03-14"})
pat = re.compile(r'^\S+ \S+ \S+ \[[^\]]*\] "[^"]*" (\d{3}) (\S+) "[^"]*" "([^"]*)"')
totals = {"bot": 0, "script": 0, "browser": 0}
unparsed = 0
for line in log.splitlines():
    if not line.strip():
        continue
    m = pat.match(line)
    if not m:
        unparsed += 1
        continue
    size, ua = m.group(2), m.group(3)
    b = int(size) if size.isdigit() else 0  # "-" means no body
    if "bot" in ua.lower():
        cls = "bot"
    elif ua.startswith("curl") or ua.startswith("python-requests"):
        cls = "script"
    else:
        cls = "browser"
    totals[cls] += b
print(totals, "unparsed:", unparsed)
print(await submit_answer({"bytes": totals}))
