import re
from datetime import datetime

text = await read_file({"path": "/docs/contract.txt"})
months = r"January|February|March|April|May|June|July|August|September|October|November|December"
date_re = rf"(?:\d{{4}}-\d{{2}}-\d{{2}}|(?:{months})\s+\d{{1,2}},\s*\d{{4}})"

def date_value(value):
    value = value.strip()
    parsed = datetime.strptime(value, "%Y-%m-%d" if value[0].isdigit() else "%B %d, %Y")
    return parsed.strftime("%Y-%m-%d")

def number_before(pattern, default=None):
    match = re.search(pattern, text, re.I)
    return float(match.group(1).replace(",", "")) if match else default

fee = number_before(r"quarterly\s+(?:service\s+)?fee\D{0,30}\$?([\d,]+(?:\.\d+)?)")
late = number_before(r"(?:monthly\s+)?late\s+fee\D{0,30}(\d+(?:\.\d+)?)\s*%")
days_match = re.search(r"(?:payment|pay(?:ment)?\s+due)\D{0,35}(\d+)\s+days", text, re.I)
all_dates = [date_value(m.group()) for m in re.finditer(date_re, text, re.I)]
start_match = re.search(rf"(?:start(?:s|ing)?|commenc\w*|effective\s+date)\D{{0,80}}({date_re})", text, re.I)
end_match = re.search(rf"(?:end(?:s|ing)?|expir\w*|terminat\w*|through)\D{{0,80}}({date_re})", text, re.I)
start = date_value(start_match.group(1)) if start_match else (all_dates[0] if all_dates else "")
end = date_value(end_match.group(1)) if end_match else (all_dates[-1] if len(all_dates) > 1 else start)
await submit_answer({
    "quarterly_fee_usd": fee,
    "late_fee_pct": late,
    "payment_days": int(days_match.group(1)) if days_match else 0,
    "start": start,
    "end": end,
})
