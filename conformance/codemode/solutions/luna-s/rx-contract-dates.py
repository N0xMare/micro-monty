import re
from datetime import datetime

text = await read_file({"path": "/docs/contract.txt"})
months = r"January|February|March|April|May|June|July|August|September|October|November|December"
pattern = re.compile(rf"\b(\d{{4}}-\d{{2}}-\d{{2}}|(?:{months})\s+\d{{1,2}},\s*\d{{4}})\b", re.I)
dates = []
for match in pattern.finditer(text):
    value = match.group(1)
    try:
        parsed = datetime.strptime(value, "%Y-%m-%d" if value[0].isdigit() else "%B %d, %Y")
        dates.append(parsed.strftime("%Y-%m-%d"))
    except ValueError:
        pass
await submit_answer({"dates": dates})
