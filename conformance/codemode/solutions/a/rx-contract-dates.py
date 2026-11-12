import re
from datetime import date

text = await read_file({"path": "/docs/contract.txt"})

months = {
    "january": 1, "february": 2, "march": 3, "april": 4, "may": 5, "june": 6,
    "july": 7, "august": 8, "september": 9, "october": 10, "november": 11, "december": 12,
    "jan": 1, "feb": 2, "mar": 3, "apr": 4, "jun": 6, "jul": 7, "aug": 8,
    "sep": 9, "sept": 9, "oct": 10, "nov": 11, "dec": 12,
}
month_alt = "|".join(sorted(months, key=len, reverse=True))

pattern = re.compile(
    r"\b(?P<iso>(?P<y>\d{4})-(?P<m>\d{1,2})-(?P<d>\d{1,2}))\b"
    r"|\b(?P<mon>" + month_alt + r")\.?\s+(?P<day>\d{1,2})(?:st|nd|rd|th)?,?\s+(?P<year>\d{4})\b",
    re.IGNORECASE,
)

dates = []
for m in pattern.finditer(text):
    try:
        if m.group("iso"):
            d = date(int(m.group("y")), int(m.group("m")), int(m.group("d")))
        else:
            d = date(int(m.group("year")), months[m.group("mon").lower()], int(m.group("day")))
    except ValueError:
        continue
    dates.append(d.isoformat())

print(dates)
print(await submit_answer({"dates": dates}))
