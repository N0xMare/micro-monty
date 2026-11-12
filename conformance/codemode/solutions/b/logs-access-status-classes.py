import re

text = await get_access_log({"date": "2026-03-14"})
pat = re.compile(r'^(\S+) \S+ \S+ \[([^\]]+)\] "([^"]*)" (\d{3}) (\S+)')
classes = {"2xx": 0, "3xx": 0, "4xx": 0, "5xx": 0}
unparsed = 0
for line in text.splitlines():
    if not line.strip():
        continue
    m = pat.match(line)
    if not m:
        unparsed += 1
        continue
    key = m.group(4)[0] + "xx"
    if key in classes:
        classes[key] += 1
print(classes, "unparsed:", unparsed)
print(await submit_answer({"classes": classes}))
