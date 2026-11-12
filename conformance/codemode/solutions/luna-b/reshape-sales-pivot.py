import csv, io
from collections import defaultdict

text = await read_file({"path": "/data/sales_2026q1.csv"})
months = ["2026-01", "2026-02", "2026-03"]
pivot = defaultdict(lambda: {month: 0.0 for month in months})
for row in csv.DictReader(io.StringIO(text)):
    month = row["date"][:7]
    pivot[row["rep"]][month] += int(row["units"]) * float(row["unit_price"])
result = {rep: {month: round(values[month] + 1e-9, 2) for month in months} for rep, values in pivot.items()}
await submit_answer({"pivot": result})
