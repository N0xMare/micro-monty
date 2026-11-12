import re
from datetime import datetime

text = await read_file({"path": "/docs/contract.txt"})
money = r"(?:\$|USD\s*)([\d,]+(?:\.\d+)?)"
fee_match = re.search(r"quarterly\s+fee[^\n$\d]*" + money, text, re.I)
if fee_match is None:
    fee_match = re.search(money + r"\s*(?:per\s+quarter|quarterly)", text, re.I)
late_match = re.search(r"(?:monthly\s+late\s+fee|late\s+fee)[^\n\d]*([\d.]+)\s*%", text, re.I)
if late_match is None:
    late_match = re.search(r"([\d.]+)\s*%[^\n.]*\b(?:month|monthly)\b", text, re.I)
days_match = re.search(r"(?:within|net|payment\s+(?:term|terms)(?:\s+of)?\s*:?)[^\n\d]*(\d+)\s*(?:days)?", text, re.I)
date_pattern = r"\b(?:\d{4}-\d{2}-\d{2}|(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},\s+\d{4})\b"
term = re.search(r"\bterm\b[^\n]*", text, re.I)
term_dates = re.findall(date_pattern, term.group() if term else text)
if len(term_dates) < 2:
    term_dates = re.findall(date_pattern, text)
def normalize(value):
    return datetime.strptime(value, "%Y-%m-%d" if value[0].isdigit() else "%B %d, %Y").date().isoformat()
await submit_answer({"quarterly_fee_usd": float(fee_match[1].replace(",", "")), "late_fee_pct": float(late_match[1]), "payment_days": int(days_match[1]), "start": normalize(term_dates[0]), "end": normalize(term_dates[1])})
