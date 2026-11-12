import re
from collections import Counter
from urllib.parse import urlsplit, parse_qs

log = await get_access_log({"date": "2026-03-14"})
terms = Counter()
for line in log.splitlines():
    match = re.search(r'\] "\S+ (\S+) HTTP/[^\"]+"', line)
    if not match:
        continue
    url = urlsplit(match.group(1))
    if url.path == "/search":
        terms.update(parse_qs(url.query, keep_blank_values=True).get("q", []))
await submit_answer({"terms": dict(terms)})
