import csv, io
from collections import defaultdict

text = (await read_file({"path": "/data/bank_feb.csv"})).lstrip("\ufeff")
lines = text.splitlines()
# Skip any preamble before the header row.
start = next(i for i, l in enumerate(lines) if "verwendungszweck" in l.lower())
reader = csv.reader(io.StringIO("\n".join(lines[start:])), delimiter=";")
header = [h.strip().lower() for h in next(reader)]
print(header)

def col(*keys):
    for k in keys:
        for i, h in enumerate(header):
            if k in h:
                return i
    raise KeyError(keys)

i_purpose = col("verwendungszweck")
i_amount = col("betrag", "amount")
i_date = col("buchungstag", "buchungsdatum", "datum", "valuta", "date")

def parse_amount(s):
    s = s.strip().replace("€", "").replace("EUR", "").replace(" ", "").replace("\u00a0", "")
    s = s.replace(".", "").replace(",", ".")
    return float(s)

def parse_date(s):
    d, m, y = s.strip().split(".")
    return f"{y}-{int(m):02d}-{int(d):02d}"

totals = defaultdict(float)
largest = None  # (amount, date)
for row in reader:
    if not row or all(not c.strip() for c in row):
        continue
    amt = parse_amount(row[i_amount])
    purpose = row[i_purpose].strip()
    totals[purpose] += amt
    if amt < 0 and (largest is None or amt < largest[0]):
        largest = (amt, parse_date(row[i_date]))

totals = {k: round(v, 2) for k, v in sorted(totals.items())}
print(totals, largest)
print(await submit_answer({"totals": totals, "largest_debit_date": largest[1]}))
