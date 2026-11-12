import re
from datetime import date

text = await read_file({"path": "/docs/contract.txt"})

MONTHS = {
    "jan": 1, "feb": 2, "mar": 3, "apr": 4, "may": 5, "jun": 6,
    "jul": 7, "aug": 8, "sep": 9, "sept": 9, "oct": 10, "nov": 11, "dec": 12,
}
month_names = (r"January|February|March|April|May|June|July|August|September|October|November|December"
               r"|Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sept|Sep|Oct|Nov|Dec")
pattern = re.compile(
    r"\b(?P<iso>(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2}))\b"
    r"|\b(?P<mon>" + month_names + r")\.?\s+(?P<day>\d{1,2})(?:st|nd|rd|th)?,?\s+(?P<year>\d{4})\b",
    re.I,
)

dates = []
for m in pattern.finditer(text):
    try:
        if m.group("iso"):
            d = date(int(m.group("y")), int(m.group("m")), int(m.group("d")))
        else:
            d = date(int(m.group("year")), MONTHS[m.group("mon").lower()[:4].rstrip("t") if m.group("mon").lower().startswith("sept") else m.group("mon").lower()[:3]], int(m.group("day")))
    except ValueError:
        print("skipping invalid date:", m.group(0))
        continue
    dates.append(d.isoformat())

print(dates)
print(await submit_answer({"dates": dates}))
