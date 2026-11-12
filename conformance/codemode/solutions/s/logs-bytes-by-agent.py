import re

log = await get_access_log({"date": "2026-03-14"})
pat = re.compile(r'^(\S+) \S+ \S+ \[([^\]]*)\] "([^"]*)" (\d{3}) (\S+)(?: "([^"]*)" "([^"]*)")?')
totals = {"bot": 0, "script": 0, "browser": 0}
unparsed = 0
for line in log.splitlines():
    if not line.strip():
        continue
    m = pat.match(line)
    if not m:
        unparsed += 1
        continue
    b = m.group(5)
    nbytes = int(b) if b.isdigit() else 0
    ua = m.group(7) or ""
    if "bot" in ua.lower():
        cls = "bot"
    elif ua.startswith("curl") or ua.startswith("python-requests"):
        cls = "script"
    else:
        cls = "browser"
    totals[cls] += nbytes
print(totals, "unparsed:", unparsed)
print(await submit_answer({"bytes": totals}))
