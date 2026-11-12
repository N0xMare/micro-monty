import re
from datetime import datetime
text = await read_file({"path": "/docs/contract.txt"})
date_pattern = r"\d{4}-\d{2}-\d{2}|(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},\s+\d{4}"
def normalize(value):
    return datetime.strptime(value, "%Y-%m-%d" if re.match(r"\d{4}-", value) else "%B %d, %Y").strftime("%Y-%m-%d")
fee = re.search(r"quarterly\s+fee[^\d$]*\$?\s*([\d,]+(?:\.\d+)?)", text, re.I)
if fee is None:
    fee = re.search(r"\$\s*([\d,]+(?:\.\d+)?)\s*(?:per\s+quarter|quarterly)", text, re.I)
late = re.search(r"late\s+fee[^\d]*([\d.]+)\s*%", text, re.I)
if late is None:
    late = re.search(r"([\d.]+)\s*%[^.\n]*\b(?:month|monthly)\b", text, re.I)
payment = re.search(r"(?:payment|payable|due)[^.\n]*?\b(\d+)\s+days\b", text, re.I)
if payment is None:
    payment = re.search(r"\bnet\s+(\d+)\b", text, re.I)
term = re.search(r"\bterm\b[^\n]*", text, re.I)
term_dates = re.findall(date_pattern, term.group() if term else text)
if len(term_dates) < 2:
    start_match = re.search(r"(?:start|commenc|begin|effective)[\s\S]*?(" + date_pattern + r")", text, re.I)
    end_match = re.search(r"(?:end|expir|terminat)[\s\S]*?(" + date_pattern + r")", text, re.I)
    term_dates = [start_match.group(1), end_match.group(1)]
await submit_answer({"quarterly_fee_usd": float(fee.group(1).replace(",", "")), "late_fee_pct": float(late.group(1)), "payment_days": int(payment.group(1)), "start": normalize(term_dates[0]), "end": normalize(term_dates[1])})
