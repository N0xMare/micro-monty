import re
from collections import Counter
from urllib.parse import urlsplit, parse_qsl

log = await get_access_log({"date": "2026-03-14"})
req_re = re.compile(r'"(?:[A-Z]+)\s+(\S+)(?:\s+HTTP/[\d.]+)?"')

params = Counter()
lines = 0
for line in log.splitlines():
    m = req_re.search(line)
    if not m:
        continue
    lines += 1
    query = urlsplit(m.group(1)).query
    if not query:
        continue
    names = {k for k, _ in parse_qsl(query, keep_blank_values=True) if k}
    params.update(names)  # count each request once per parameter name

print(lines, "requests parsed")
result = dict(sorted(params.items()))
print(result)
print(await submit_answer({"params": result}))
