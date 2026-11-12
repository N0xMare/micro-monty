import re

log = await get_access_log({"date": "2026-03-14"})
paths = set()
for line in log.splitlines():
    match = re.search(r'\] "\S+ (\S+) HTTP/[^\"]+"\s+(\d{3})\s', line)
    if match and match.group(2) == "404":
        paths.add(match.group(1).split("?", 1)[0])
await submit_answer({"paths": sorted(paths)})
