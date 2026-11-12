import asyncio
import re

text = await get_access_log({"date": "2026-03-14"})
classes = {f"{hundreds}xx": 0 for hundreds in range(2, 6)}
for line in text.splitlines():
    match = re.search(r'"[^\"]*"\s+(\d{3})\s+', line)
    if match:
        status_class = f"{match.group(1)[0]}xx"
        if status_class in classes:
            classes[status_class] += 1
await submit_answer({"classes": classes})
