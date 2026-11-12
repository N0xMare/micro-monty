import csv, io
from collections import defaultdict

raw = await read_file({"path": "/data/sales_2026q1.csv"})
rows = list(csv.DictReader(io.StringIO(raw)))
cols = list(rows[0].keys()) if rows else []
print("columns:", cols, "rows:", len(rows))
print("sample:", rows[:3])

def pick(*cands):
    low = {c.lower().strip(): c for c in cols}
    for c in cands:
        if c in low:
            return low[c]
    raise KeyError(f"none of {cands} in {cols}")

region_col = pick("region")
units_col = pick("units", "qty", "quantity", "units_sold")
try:
    month_col, is_month = pick("month"), True
except KeyError:
    month_col, is_month = pick("date", "order_date", "sale_date", "day"), False

pivot = defaultdict(lambda: defaultdict(int))
months = set()
for r in rows:
    region = r[region_col].strip()
    m = r[month_col].strip()[:7]  # YYYY-MM from YYYY-MM or YYYY-MM-DD
    u = r[units_col].strip()
    units = int(float(u)) if u else 0
    months.add(m)
    pivot[region][m] += units

months = sorted(months)
print("months:", months)
pivot_out = {reg: {m: pivot[reg].get(m, 0) for m in months} for reg in sorted(pivot)}
grand = sum(sum(v.values()) for v in pivot_out.values())
share = {reg: round(100 * sum(v.values()) / grand, 1) if grand else 0.0 for reg, v in pivot_out.items()}
for reg in pivot_out:
    print(reg, pivot_out[reg], share[reg])
print(await submit_answer({"pivot": pivot_out, "share": share}))
