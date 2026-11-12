from collections import Counter

text = await get_access_log({"date": "2026-03-14"})
counts = Counter()
for line in text.splitlines():
    fields = line.split()
    status = int(fields[8])
    counts[f"{status // 100}xx"] += 1
classes = {key: counts[key] for key in ("2xx", "3xx", "4xx", "5xx")}
await submit_answer({"classes": classes})
