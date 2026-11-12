import re
from collections import Counter

text = await get_access_log({"date": "2026-03-14"})
classes = Counter({"2xx": 0, "3xx": 0, "4xx": 0, "5xx": 0})
for line in text.splitlines():
    match = re.search(r'"[^\"]*"\s+(\d{3})\s+', line)
    if match:
        classes[f"{match.group(1)[0]}xx"] += 1
await submit_answer({"classes": dict(classes)})
