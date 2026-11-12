import re
log = await get_access_log({"date": "2026-03-14"}); paths = set()
for line in log.splitlines():
    m = re.search(r'"\S+\s+(\S+)\s+HTTP/[^\"]+"\s+(\d{3})\b', line)
    if m and int(m.group(2)) == 404: paths.add(m.group(1).split("?", 1)[0])
await submit_answer({"paths": sorted(paths)})
