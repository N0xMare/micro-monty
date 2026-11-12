import csv, io
from collections import defaultdict
from datetime import datetime

text = await read_file({"path": "/data/bank_feb.csv"})
text = text.lstrip("\ufeff")
lines = [l for l in text.splitlines() if l.strip()]
print(lines[:5])

# Find the header row (the first line mentioning Verwendungszweck)
start = next(i for i, l in enumerate(lines) if "verwendungszweck" in l.lower())
reader = csv.reader(io.StringIO("\n".join(lines[start:])), delimiter=";")
header = [h.strip() for h in next(reader)]

def col(*keys):
    for i, h in enumerate(header):
        if any(k in h.lower() for k in keys):
            return i
    raise KeyError(keys)

i_purpose = col("verwendungszweck")
i_amount = col("betrag", "amount", "umsatz")
i_date = col("buchungstag", "buchungsdatum", "datum", "date", "valuta")

def parse_amount(s):
    s = s.strip().replace("€", "").replace("EUR", "").replace(" ", "").replace("\u00a0", "")
    s = s.replace(".", "").replace(",", ".")
    return float(s)

totals = defaultdict(float)
largest = None  # (amount, date)
for row in reader:
    if len(row) <= max(i_purpose, i_amount, i_date) or not row[i_amount].strip():
        continue
    amt = parse_amount(row[i_amount])
    purpose = row[i_purpose].strip()
    totals[purpose] += amt
    if amt < 0 and (largest is None or amt < largest[0]):
        d = datetime.strptime(row[i_date].strip(), "%d.%m.%Y").date().isoformat()
        largest = (amt, d)

totals = {k: round(v, 2) for k, v in sorted(totals.items())}
print(totals, largest)
await submit_answer({"totals": totals, "largest_debit_date": largest[1]})
