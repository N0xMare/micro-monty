import re
from collections import Counter
from urllib.parse import urlsplit, parse_qs

log = await get_access_log({"date": "2026-03-14"})
req_re = re.compile(r'"([A-Z]+) (\S+)(?: [^"]*)?"')
terms = Counter()
for line in log.splitlines():
    m = req_re.search(line)
    if not m:
        continue
    parts = urlsplit(m.group(2))
    if parts.path != "/search":
        continue
    qs = parse_qs(parts.query, keep_blank_values=True)
    for v in qs.get("q", []):
        terms[v] += 1
terms = dict(terms.most_common())
print(terms)
print(await submit_answer({"terms": terms}))
