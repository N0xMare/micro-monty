import re
from collections import Counter
from urllib.parse import urlsplit, parse_qs
log = await get_access_log({"date": "2026-03-14"}); terms = Counter()
for line in log.splitlines():
    m = re.search(r'"\S+\s+(\S+)\s+HTTP/', line)
    if m and urlsplit(m.group(1)).path == "/search": terms.update(parse_qs(urlsplit(m.group(1)).query, keep_blank_values=True).get("q", []))
await submit_answer({"terms": dict(terms)})
