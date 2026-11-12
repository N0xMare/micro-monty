import re
from collections import Counter
from urllib.parse import urlsplit, parse_qsl

log = await get_access_log({"date": "2026-03-14"})

req_re = re.compile(r'"[A-Z]+ (\S+)(?: HTTP/[\d.]+)?"')
counts = Counter()
n_lines = n_parsed = 0
for line in log.splitlines():
    if not line.strip():
        continue
    n_lines += 1
    m = req_re.search(line)
    if not m:
        print("unparsed:", line)
        continue
    n_parsed += 1
    query = urlsplit(m.group(1)).query
    if not query:
        continue
    names = {name for name, _ in parse_qsl(query, keep_blank_values=True) if name}
    counts.update(names)

print(n_lines, n_parsed)
params = dict(sorted(counts.items()))
print(params)
await submit_answer({"params": params})
