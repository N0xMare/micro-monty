import re
from datetime import datetime

text = await read_file({"path": "/docs/contract.txt"})
months = "January February March April May June July August September October November December".split()
pattern = r"\b(?:\d{4}-\d{2}-\d{2}|(?:" + "|".join(months) + r")\s+\d{1,2},\s+\d{4})\b"
dates = []
for match in re.finditer(pattern, text):
    value = match.group()
    fmt = "%Y-%m-%d" if re.match(r"\d{4}-", value) else "%B %d, %Y"
    dates.append(datetime.strptime(re.sub(r"\s+", " ", value), fmt).date().isoformat())
await submit_answer({"dates": dates})
