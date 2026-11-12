import re
from datetime import datetime

text = await read_file({"path": "/docs/contract.txt"})
def number(pattern):
    m = re.search(pattern, text, re.I)
    return float(m.group(1).replace(",", "")) if m else None

fee = number(r"quarterly(?:\s+service)?\s+fee\D{0,30}\$?([\d,]+(?:\.\d{1,2})?)")
late = number(r"monthly\s+late\s+fee\D{0,20}([\d.]+)\s*%")
days_match = re.search(r"payment\s+(?:term|due)\D{0,20}(\d+)\s+days", text, re.I)
dates_re = re.compile(r"\b(\d{4}-\d{2}-\d{2}|(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},\s+\d{4})\b", re.I)
dates = []
for m in dates_re.finditer(text):
    s = m.group(1)
    dates.append(datetime.strptime(s, "%Y-%m-%d" if s[4:5] == "-" else "%B %d, %Y").strftime("%Y-%m-%d"))
await submit_answer({"quarterly_fee_usd": fee, "late_fee_pct": late, "payment_days": int(days_match.group(1)) if days_match else None, "start": dates[0] if dates else None, "end": dates[1] if len(dates) > 1 else None})
