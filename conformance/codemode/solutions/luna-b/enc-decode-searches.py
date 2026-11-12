import json, re
from collections import Counter
from urllib.parse import parse_qs, urlsplit

log = await get_access_log({"date": "2026-03-14"})
terms = Counter()
for line in log.splitlines():
    m = re.search(r'"\S+\s+(\S+)\s+HTTP/[^\"]+"', line)
    if not m:
        continue
    target = urlsplit(m.group(1))
    if target.path == "/search":
        for q in parse_qs(target.query, keep_blank_values=True).get("q", []):
            terms[q] += 1
await submit_answer({"terms": dict(terms)})
