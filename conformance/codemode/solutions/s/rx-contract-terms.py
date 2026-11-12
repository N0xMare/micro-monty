import re
import datetime

text = await read_file({"path": "/docs/contract.txt"})

MONTHS = {
    "jan": 1, "feb": 2, "mar": 3, "apr": 4, "may": 5, "jun": 6,
    "jul": 7, "aug": 8, "sep": 9, "oct": 10, "nov": 11, "dec": 12,
}
DATE_RE = re.compile(
    r'\b(\d{4})-(\d{2})-(\d{2})\b'
    r'|\b(Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|June?|July?|Aug(?:ust)?'
    r'|Sep(?:t(?:ember)?)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)\.?\s+(\d{1,2})(?:st|nd|rd|th)?,?\s+(\d{4})\b',
    re.IGNORECASE,
)

def find_dates(s):
    out = []
    for m in DATE_RE.finditer(s):
        try:
            if m.group(1):
                d = datetime.date(int(m.group(1)), int(m.group(2)), int(m.group(3)))
            else:
                d = datetime.date(int(m.group(6)), MONTHS[m.group(4)[:3].lower()], int(m.group(5)))
        except ValueError:
            continue
        out.append((m.start(), d.isoformat()))
    return out

def num(s):
    return float(s.replace(",", ""))

# Split into sentences / clauses (a period followed by whitespace, or blank lines).
flat = re.sub(r'[ \t]*\n[ \t]*', ' ', text)
sentences = [s.strip() for s in re.split(r'(?<=[.;])\s+(?=[A-Z0-9(])|\n\s*\n', flat) if s.strip()]
for s in sentences:
    print("|", s)

AMOUNT_RE = re.compile(r'(?:\$|USD\s?|US\$)\s?([\d,]+(?:\.\d+)?)|([\d,]+(?:\.\d+)?)\s?(?:USD|dollars)', re.I)

# Quarterly fee
quarterly_fee = None
for prefer_fee in (True, False):
    for s in sentences:
        low = s.lower()
        if "quarter" in low and (not prefer_fee or "fee" in low):
            m = AMOUNT_RE.search(s)
            if m:
                quarterly_fee = num(m.group(1) or m.group(2))
                break
    if quarterly_fee is not None:
        break

# Late fee percentage (monthly)
late_fee = None
for s in sentences:
    low = s.lower()
    if "late" in low or "overdue" in low or "past due" in low:
        pcts = list(re.finditer(r'(\d+(?:\.\d+)?)\s?(?:%|percent)', s, re.I))
        if pcts:
            chosen = pcts[0]
            for p in pcts:
                after = s[p.end():p.end() + 20].lower()
                if "month" in after:
                    chosen = p
                    break
            late_fee = float(chosen.group(1))
            break

# Payment term in days
payment_days = None
for s in sentences:
    low = s.lower()
    m_net = re.search(r'\bnet[\s-]?(\d+)\b', s, re.I)
    if m_net:
        payment_days = int(m_net.group(1))
        break
    if ("pay" in low or "invoice" in low) and "day" in low:
        m = re.search(r'(?:\((\d+)\)|\b(\d+))\s*(?:calendar\s+|business\s+)?days?\b', s, re.I)
        if m:
            payment_days = int(m.group(1) or m.group(2))
            break

# Term start and end
start = end = None
for s in sentences:
    low = s.lower()
    ds = find_dates(s)
    if re.search(r'\bterm\b', low) and len(ds) >= 2:
        start, end = ds[0][1], ds[1][1]
        break
if start is None:
    for s in sentences:
        low = s.lower()
        ds = find_dates(s)
        if not ds:
            continue
        if start is None and re.search(r'\b(commenc\w*|begin\w*|start\w*|effective)\b', low):
            start = ds[0][1]
        if end is None and re.search(r'\b(end\w*|expir\w*|terminat\w*|until|through)\b', low):
            end = ds[-1][1]

answer = {
    "quarterly_fee_usd": quarterly_fee,
    "late_fee_pct": late_fee,
    "payment_days": payment_days,
    "start": start,
    "end": end,
}
print(answer)
await submit_answer(answer)
