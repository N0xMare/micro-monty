import re
from datetime import datetime
text = await read_file({"path": "/docs/contract.txt"})
pattern = r"\b(?:\d{4}-\d{2}-\d{2}|(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},\s+\d{4})\b"
dates = []
for match in re.finditer(pattern, text):
    value = match.group()
    dates.append(datetime.strptime(value, "%Y-%m-%d" if value[0].isdigit() else "%B %d, %Y").strftime("%Y-%m-%d"))
await submit_answer({"dates": dates})
