import csv, io
from collections import defaultdict
from decimal import Decimal, ROUND_HALF_UP
from datetime import datetime

text = await read_file({"path": "/data/sales_2026q1.csv"})
rows = list(csv.DictReader(io.StringIO(text.lstrip("﻿"))))
rows = [{(k or "").strip().lower(): (v or "").strip() for k, v in r.items()} for r in rows]

months = ["2026-01", "2026-02", "2026-03"]

def month_of(d):
    for fmt in ("%Y-%m-%d", "%Y/%m/%d", "%m/%d/%Y", "%d.%m.%Y", "%Y-%m-%dT%H:%M:%S"):
        try:
            return datetime.strptime(d, fmt).strftime("%Y-%m")
        except ValueError:
            pass
    return d[:7]

pivot = defaultdict(lambda: {m: Decimal(0) for m in months})
other = []
for r in rows:
    if not r.get("rep"):
        continue
    m = month_of(r["date"])
    rev = Decimal(r["units"].replace(",", "")) * Decimal(r["unit_price"].replace(",", "").lstrip("$"))
    if m not in months:
        other.append((r["date"], r["rep"]))
        continue
    pivot[r["rep"]][m] += rev

if other:
    print("rows outside Q1:", other)
result = {
    rep: {m: float(v.quantize(Decimal("0.01"), rounding=ROUND_HALF_UP)) for m, v in vals.items()}
    for rep, vals in sorted(pivot.items())
}
print(result)
print(await submit_answer({"pivot": result}))
