import re
import datetime

text = await read_file({"path": "/docs/contract.txt"})

MONTHS = {
    "jan": 1, "feb": 2, "mar": 3, "apr": 4, "may": 5, "jun": 6,
    "jul": 7, "aug": 8, "sep": 9, "oct": 10, "nov": 11, "dec": 12,
}

pattern = re.compile(
    r'\b(\d{4})-(\d{2})-(\d{2})\b'
    r'|\b(Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|June?|July?|Aug(?:ust)?'
    r'|Sep(?:t(?:ember)?)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)\.?\s+(\d{1,2})(?:st|nd|rd|th)?,?\s+(\d{4})\b',
    re.IGNORECASE,
)

dates = []
for m in pattern.finditer(text):
    try:
        if m.group(1):
            d = datetime.date(int(m.group(1)), int(m.group(2)), int(m.group(3)))
        else:
            d = datetime.date(int(m.group(6)), MONTHS[m.group(4)[:3].lower()], int(m.group(5)))
    except ValueError:
        print("skipping invalid date:", m.group(0))
        continue
    dates.append(d.isoformat())

print(len(dates), dates)
await submit_answer({"dates": dates})
