import re
from collections import Counter
from urllib.parse import urlsplit, parse_qsl, unquote_plus

log = await get_access_log({"date": "2026-03-14"})

req_re = re.compile(r'"[A-Z]+ (\S+)(?: HTTP/[0-9.]+)?"')
counts = Counter()
for line in log.splitlines():
    m = req_re.search(line)
    if not m:
        continue
    query = urlsplit(m.group(1)).query
    if not query:
        continue
    names = set()
    for part in query.split("&"):
        if not part:
            continue
        name = unquote_plus(part.split("=", 1)[0])
        if name:
            names.add(name)
    counts.update(names)  # count each request once per parameter name

params = dict(sorted(counts.items()))
print(params)
print(await submit_answer({"params": params}))
