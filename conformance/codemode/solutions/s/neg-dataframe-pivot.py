import csv
import io
from collections import defaultdict

text = await read_file({"path": "/data/sales_2026q1.csv"})
reader = csv.DictReader(io.StringIO(text))
fields = reader.fieldnames or []
print("columns:", fields)
lower = {f.strip().lower(): f for f in fields}

def pick(*names):
    for n in names:
        if n in lower:
            return lower[n]
    return None

region_col = pick("region")
units_col = pick("units", "qty", "quantity")
month_col = pick("month")
date_col = pick("date", "order_date", "sale_date", "day")
assert region_col and units_col and (month_col or date_col), fields

pivot = defaultdict(lambda: defaultdict(int))
months = set()
for row in reader:
    region = row[region_col].strip()
    raw = (row[month_col] if month_col else row[date_col]).strip()
    month = raw[:7]
    units_raw = row[units_col].strip()
    units = int(float(units_raw)) if units_raw else 0
    months.add(month)
    pivot[region][month] += units

months = sorted(months)
pivot = {r: {m: pivot[r].get(m, 0) for m in months} for r in sorted(pivot)}
grand = sum(sum(v.values()) for v in pivot.values())
share = {r: round(100 * sum(v.values()) / grand, 1) if grand else 0.0 for r, v in pivot.items()}

print("region".ljust(12), *(m.rjust(8) for m in months), "share%".rjust(8))
for r, v in pivot.items():
    print(r.ljust(12), *(str(v[m]).rjust(8) for m in months), str(share[r]).rjust(8))
print(await submit_answer({"pivot": pivot, "share": share}))
