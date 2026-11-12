import re

log = await get_access_log({"date": "2026-03-14"})
line_re = re.compile(r'"(?:[A-Z]+) (\S+)(?: [^"]*)?" (\d{3}) ')
paths = set()
for line in log.splitlines():
    m = line_re.search(line)
    if m and m.group(2) == "404":
        paths.add(m.group(1).split("?", 1)[0])
result = sorted(paths)
print(result)
await submit_answer({"paths": result})
