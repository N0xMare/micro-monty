import re
from datetime import date

text = await read_file({"path": "/docs/contract.txt"})
print(text)

flat = re.sub(r"\s+", " ", text)

months = {
    "january": 1, "february": 2, "march": 3, "april": 4, "may": 5, "june": 6,
    "july": 7, "august": 8, "september": 9, "october": 10, "november": 11, "december": 12,
    "jan": 1, "feb": 2, "mar": 3, "apr": 4, "jun": 6, "jul": 7, "aug": 8,
    "sep": 9, "sept": 9, "oct": 10, "nov": 11, "dec": 12,
}
month_alt = "|".join(sorted(months, key=len, reverse=True))
date_re = re.compile(
    r"\b(?P<y>\d{4})-(?P<m>\d{1,2})-(?P<d>\d{1,2})\b"
    r"|\b(?P<mon>" + month_alt + r")\.?\s+(?P<day>\d{1,2})(?:st|nd|rd|th)?,?\s+(?P<year>\d{4})\b",
    re.IGNORECASE,
)


def parse_date(m):
    if m.group("y"):
        return date(int(m.group("y")), int(m.group("m")), int(m.group("d"))).isoformat()
    return date(int(m.group("year")), months[m.group("mon").lower()], int(m.group("day"))).isoformat()


def money(s):
    return float(s.replace(",", ""))


amount = r"(?:USD|US\$|\$)\s?([0-9][0-9,]*(?:\.[0-9]+)?)|([0-9][0-9,]*(?:\.[0-9]+)?)\s?(?:USD|dollars)"

# Quarterly fee: an amount near "quarter"
quarterly_fee = None
for m in re.finditer(amount, flat, re.IGNORECASE):
    window = flat[max(0, m.start() - 120): m.end() + 60].lower()
    if "quarter" in window:
        quarterly_fee = money(m.group(1) or m.group(2))
        break

# Late fee: a percentage near "late" / "month" / "overdue"
late_fee_pct = None
for m in re.finditer(r"([0-9]+(?:\.[0-9]+)?)\s?(?:%|percent)", flat, re.IGNORECASE):
    window = flat[max(0, m.start() - 150): m.end() + 80].lower()
    if ("late" in window or "overdue" in window or "interest" in window) and "month" in window:
        late_fee_pct = float(m.group(1))
        break
if late_fee_pct is None:
    for m in re.finditer(r"([0-9]+(?:\.[0-9]+)?)\s?(?:%|percent)", flat, re.IGNORECASE):
        window = flat[max(0, m.start() - 150): m.end() + 80].lower()
        if "late" in window or "overdue" in window:
            late_fee_pct = float(m.group(1))
            break

# Payment term in days
payment_days = None
m = re.search(r"\bnet[ -]?(\d+)\b", flat, re.IGNORECASE)
for m2 in re.finditer(r"(?:\(?\b(\d+)\)?|\b[a-z-]+\s+\((\d+)\))\s+(?:calendar\s+|business\s+)?days", flat, re.IGNORECASE):
    window = flat[max(0, m2.start() - 150): m2.end() + 80].lower()
    if any(k in window for k in ("pay", "invoice", "due")):
        payment_days = int(m2.group(1) or m2.group(2))
        break
if payment_days is None and m:
    payment_days = int(m.group(1))

# Term start and end dates
start = end = None
all_dates = []
for dm in date_re.finditer(flat):
    try:
        all_dates.append((dm.start(), dm.end(), parse_date(dm)))
    except ValueError:
        pass

for s, e, d in all_dates:
    before = flat[max(0, s - 80): s].lower()
    if start is None and any(k in before for k in ("commenc", "start", "effective", "begin", "from")):
        start = d
    elif end is None and any(k in before for k in ("end", "expir", "terminat", "until", "through", "to ")):
        if start is not None and d != start:
            end = d

# Fallback: a "Term" sentence with two dates
if start is None or end is None:
    for sm in re.finditer(r"[^.]*\bterm\b[^.]*\.", flat, re.IGNORECASE):
        ds = []
        for dm in date_re.finditer(sm.group(0)):
            try:
                ds.append(parse_date(dm))
            except ValueError:
                pass
        if len(ds) >= 2:
            start = start or ds[0]
            end = end or ds[1]
            break

answer = {
    "quarterly_fee_usd": quarterly_fee,
    "late_fee_pct": late_fee_pct,
    "payment_days": payment_days,
    "start": start,
    "end": end,
}
print(answer)
print(await submit_answer(answer))
