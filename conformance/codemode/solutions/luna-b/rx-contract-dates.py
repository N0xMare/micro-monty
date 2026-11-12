import re
from datetime import datetime

text = await read_file({"path": "/docs/contract.txt"})
pattern = re.compile(r"\b(\d{4}-\d{2}-\d{2}|(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},\s+\d{4})\b", re.I)
dates = []
for m in pattern.finditer(text):
    value = m.group(1)
    fmt = "%Y-%m-%d" if value[4:5] == "-" else "%B %d, %Y"
    dates.append(datetime.strptime(value, fmt).strftime("%Y-%m-%d"))
await submit_answer({"dates": dates})
