import csv, io
from collections import defaultdict

text = await read_file({"path": "/data/sales_2026q1.csv"})
rows = list(csv.DictReader(io.StringIO(text)))
cols = {c.lower().strip(): c for c in rows[0].keys()} if rows else {}
print("columns:", list(cols), "rows:", len(rows))
region_col = cols.get("region")
units_col = cols.get("units") or cols.get("qty") or cols.get("quantity")
month_col = cols.get("month") or cols.get("date") or cols.get("order_date")

pivot = defaultdict(lambda: defaultdict(int))
months = set()
for r in rows:
    region = r[region_col].strip()
    month = r[month_col].strip()[:7]
    units = int(float(r[units_col]))
    months.add(month)
    pivot[region][month] += units
months = sorted(months)
pivot = {reg: {m: pivot[reg].get(m, 0) for m in months} for reg in sorted(pivot)}
grand = sum(sum(v.values()) for v in pivot.values())
share = {reg: round(100 * sum(v.values()) / grand, 1) for reg, v in pivot.items()}
print(months, pivot, share)
print(await submit_answer({"pivot": pivot, "share": share}))
