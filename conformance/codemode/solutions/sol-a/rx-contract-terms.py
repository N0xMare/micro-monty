import re
from datetime import datetime
text = await read_file({"path": "/docs/contract.txt"})
money = r"(?:USD\s*|\$)\s*([\d,]+(?:\.\d+)?)"
fee = re.search(r"quarterly\s+fee[^\n.;]*?" + money, text, re.I)
if fee is None:
    fee = re.search(money + r"\s*(?:per\s+quarter|quarterly)", text, re.I)
if fee is None:
    fee = re.search(r"quarterly[^\n]*?" + money, text, re.I)
late = re.search(r"late\s+fee[^\n%]*?(\d+(?:\.\d+)?)\s*%", text, re.I)
if late is None:
    late = re.search(r"(\d+(?:\.\d+)?)\s*%[^\n.]*?(?:late|per\s+month|monthly)", text, re.I)
days = re.search(r"(?:within|net|payment\s+(?:term|due)[^\n\d]*?)\s*(\d+)\s*days?", text, re.I)
date = r"(?:\d{4}-\d{2}-\d{2}|(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},\s+\d{4})"
term = re.search(r"(?:term|agreement)[^\n]*?(" + date + r")[^\n]*?(" + date + r")", text, re.I)
if term:
    start, end = term.groups()
else:
    start = re.search(r"(?:start|begin|effective)[^\n]*?(" + date + r")", text, re.I).group(1)
    end = re.search(r"(?:end|expire)[^\n]*?(" + date + r")", text, re.I).group(1)
def normalize(value):
    return datetime.strptime(value, "%Y-%m-%d" if value[0].isdigit() else "%B %d, %Y").strftime("%Y-%m-%d")
await submit_answer({"quarterly_fee_usd": float(fee.group(1).replace(",", "")), "late_fee_pct": float(late.group(1)), "payment_days": int(days.group(1)), "start": normalize(start), "end": normalize(end)})
