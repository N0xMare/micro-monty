import re
from datetime import datetime

text = await read_file({"path": "/docs/contract.txt"})
def first(pattern, flags=re.I):
    m = re.search(pattern, text, flags)
    return m.group(1) if m else None

fee = first(r"(?:quarterly\s+(?:service\s+)?fee)[^\d$]*\$?([\d,]+(?:\.\d{1,2})?)")
late = first(r"(?:monthly\s+)?late\s+fee[^\d%]*([\d.]+)\s*%")
days = first(r"(?:payment|payable)[^\d]{0,50}(\d+)\s+days")
date_pat = re.compile(r"\b(\d{4}-\d{2}-\d{2}|(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},\s+\d{4})\b", re.I)
dates = []
for raw in date_pat.findall(text):
    try:
        dt = datetime.strptime(raw, "%Y-%m-%d") if re.fullmatch(r"\d{4}-\d{2}-\d{2}", raw) else datetime.strptime(raw.title(), "%B %d, %Y")
        dates.append(dt.strftime("%Y-%m-%d"))
    except ValueError:
        pass
start = first(r"(?:term|service|agreement)\s+(?:commences|begins|starts|start(?:s|ing)?\s+on|from)[^\w]*(\d{4}-\d{2}-\d{2}|(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},\s+\d{4})")
end = first(r"(?:term|service|agreement)\s+(?:ends|expires|end(?:s|ing)?\s+on|through|until)[^\w]*(\d{4}-\d{2}-\d{2}|(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},\s+\d{4})")
def norm(raw):
    return (datetime.strptime(raw, "%Y-%m-%d") if re.fullmatch(r"\d{4}-\d{2}-\d{2}", raw) else datetime.strptime(raw.title(), "%B %d, %Y")).strftime("%Y-%m-%d")
await submit_answer({"quarterly_fee_usd": float(fee.replace(",", "")) if fee else 0.0, "late_fee_pct": float(late) if late else 0.0, "payment_days": int(days) if days else 0, "start": norm(start) if start else (dates[0] if dates else ""), "end": norm(end) if end else (dates[-1] if len(dates) > 1 else "")})
