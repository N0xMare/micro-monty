import re
from urllib.parse import urlsplit

log = await get_access_log({"date": "2026-03-14"})
paths = set()
for line in log.splitlines():
    match = re.search(r'"\S+\s+(\S+)\s+HTTP/[^\"]+"\s+(\d{3})\b', line)
    if match and int(match.group(2)) == 404:
        paths.add(urlsplit(match.group(1)).path)
await submit_answer({"paths": sorted(paths)})
