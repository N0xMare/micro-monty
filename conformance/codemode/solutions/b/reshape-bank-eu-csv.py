import csv, io, re

text = (await read_file({"path": "/data/bank_feb.csv"})).lstrip("\ufeff")
lines = text.splitlines()
# Bank exports often carry preamble lines; start at the header row.
start = next(i for i, l in enumerate(lines) if "verwendungszweck" in l.lower())
reader = csv.DictReader(io.StringIO("\n".join(lines[start:])), delimiter=";")
fields = reader.fieldnames
print("columns:", fields)

def col(*keys):
    for f in fields:
        if any(k in f.lower() for k in keys):
            return f
    raise KeyError(keys)

purpose_col = col("verwendungszweck")
amount_col = col("betrag", "amount", "umsatz")
date_col = col("buchung", "datum", "date", "valuta", "wert")

def amount(s):
    s = re.sub(r"[^\d,.\-+]", "", s.strip())
    return float(s.replace(".", "").replace(",", "."))

totals, largest = {}, None
for row in reader:
    raw = (row.get(amount_col) or "").strip()
    if not raw:
        continue
    a = amount(raw)
    p = row[purpose_col].strip()
    totals[p] = totals.get(p, 0.0) + a
    if a < 0 and (largest is None or a < largest[0]):
        largest = (a, row[date_col].strip())

totals = {p: round(v, 2) for p, v in totals.items()}
d, m, y = largest[1].split(".")
largest_date = f"{y}-{m.zfill(2)}-{d.zfill(2)}"

print(totals, largest)
print(await submit_answer({"totals": totals, "largest_debit_date": largest_date}))
