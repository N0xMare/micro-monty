import csv
import io
from collections import defaultdict

text = await read_file({"path": "/data/sales_2026q1.csv"})
months = ["2026-01", "2026-02", "2026-03"]
pivot = defaultdict(lambda: {m: 0.0 for m in months})
other = []
for row in csv.DictReader(io.StringIO(text)):
    row = {(k or "").strip(): (v or "").strip() for k, v in row.items()}
    if not row.get("rep"):
        continue
    month = row["date"][:7]
    revenue = float(row["units"]) * float(row["unit_price"])
    if month not in months:
        other.append(row)
        continue
    pivot[row["rep"]][month] += revenue

if other:
    print("rows outside Q1:", other)
result = {rep: {m: round(v, 2) for m, v in ms.items()} for rep, ms in sorted(pivot.items())}
print(result)
print(await submit_answer({"pivot": result}))
