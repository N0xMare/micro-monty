import re
from datetime import date

text = await read_file({"path": "/docs/contract.txt"})
flat = re.sub(r"\s+", " ", text)

MONTHS = {m: i for i, m in enumerate(
    ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"], 1)}
DATE_RE = re.compile(
    r"\b(\d{4})-(\d{2})-(\d{2})\b"
    r"|\b(January|February|March|April|May|June|July|August|September|October|November|December"
    r"|Jan|Feb|Mar|Apr|Jun|Jul|Aug|Sept|Sep|Oct|Nov|Dec)\.?\s+(\d{1,2})(?:st|nd|rd|th)?,?\s+(\d{4})\b",
    re.I,
)

def dates_in(s):
    out = []
    for m in DATE_RE.finditer(s):
        if m.group(1):
            d = date(int(m.group(1)), int(m.group(2)), int(m.group(3)))
        else:
            d = date(int(m.group(6)), MONTHS[m.group(4).lower()[:3]], int(m.group(5)))
        out.append((m.start(), d.isoformat()))
    return out

def money(s):
    return float(s.replace(",", ""))

# Quarterly fee
fee = None
for pat in [
    r"quarterly\s+(?:fee|payment|amount)[^$€£\d.]{0,60}?(?:USD|US\$|\$)\s*([\d,]+(?:\.\d+)?)",
    r"(?:USD|US\$|\$)\s*([\d,]+(?:\.\d+)?)[^.]{0,60}?(?:per|each|every|a)\s+(?:calendar\s+)?quarter",
    r"([\d,]+(?:\.\d+)?)\s*(?:USD|dollars)[^.]{0,60}?(?:per|each|every|a)\s+(?:calendar\s+)?quarter",
    r"quarter[^.]{0,80}?(?:USD|US\$|\$)\s*([\d,]+(?:\.\d+)?)",
]:
    m = re.search(pat, flat, re.I)
    if m:
        fee = money(m.group(1))
        break

# Late fee percentage per month
late = None
for pat in [
    r"([\d.]+)\s*(?:%|percent)\s*(?:per|each|a|every)\s+month",
    r"late[^.]{0,120}?([\d.]+)\s*(?:%|percent)",
    r"([\d.]+)\s*(?:%|percent)[^.]{0,80}?(?:monthly|per month)",
]:
    m = re.search(pat, flat, re.I)
    if m:
        late = float(m.group(1).rstrip("."))
        break

# Payment term in days
days = None
for pat in [
    r"(?:within|net)\s+(\d+)\s*(?:\(\w+\)\s*)?(?:calendar\s+|business\s+)?days",
    r"(\d+)\s*(?:\(\w+\)\s*)?(?:calendar\s+)?days\s+(?:of|after|from|following)[^.]{0,40}?invoice",
    r"net[\s-]?(\d+)\b",
]:
    m = re.search(pat, flat, re.I)
    if m:
        days = int(m.group(1))
        break

# Term start and end
start = end = None
sentences = re.split(r"(?<=[.;])\s+", flat)
for s in sentences:
    if re.search(r"\bterm\b", s, re.I):
        ds = dates_in(s)
        if len(ds) >= 2:
            start, end = ds[0][1], ds[1][1]
            break
if start is None or end is None:
    all_dates = dates_in(flat)
    for pos, d in all_dates:
        before = flat[max(0, pos - 80):pos].lower()
        if start is None and re.search(r"commenc|start|begin|effective|from", before):
            start = d
        elif end is None and re.search(r"\bend|expir|terminat|until|through|to\b", before):
            end = d
    if start is None and all_dates:
        start = all_dates[0][1]
    if end is None and len(all_dates) > 1:
        end = all_dates[1][1]

answer = {
    "quarterly_fee_usd": fee,
    "late_fee_pct": late,
    "payment_days": days,
    "start": start,
    "end": end,
}
print(answer)
for s in sentences:
    if re.search(r"quarter|late|days|term\b|commenc|expir", s, re.I):
        print("  >", s[:300])
print(await submit_answer(answer))
