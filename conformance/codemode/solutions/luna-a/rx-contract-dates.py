import re
from datetime import datetime

text = await read_file({"path": "/docs/contract.txt"})
pattern = re.compile(r"\b(\d{4}-\d{2}-\d{2}|(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},\s+\d{4})\b", re.I)
dates = []
for value in pattern.findall(text):
    try:
        parsed = datetime.strptime(value, "%Y-%m-%d") if re.fullmatch(r"\d{4}-\d{2}-\d{2}", value) else datetime.strptime(value.title(), "%B %d, %Y")
        dates.append(parsed.strftime("%Y-%m-%d"))
    except ValueError:
        pass
await submit_answer({"dates": dates})
