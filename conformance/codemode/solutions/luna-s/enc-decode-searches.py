import re
from collections import Counter
from urllib.parse import parse_qs, urlsplit

log = await get_access_log({"date": "2026-03-14"})
terms = Counter()
for line in log.splitlines():
    match = re.search(r'"[A-Z]+ (\S+) HTTP/[^\"]+"', line)
    if not match:
        continue
    request = urlsplit(match.group(1))
    if request.path != "/search":
        continue
    for term in parse_qs(request.query, keep_blank_values=True).get("q", []):
        terms[term] += 1
await submit_answer({"terms": dict(terms)})
