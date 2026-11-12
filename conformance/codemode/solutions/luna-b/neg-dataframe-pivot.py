import csv
import io
import json
from collections import defaultdict

text = await read_file({"path": "/data/sales_2026q1.csv"})
reader = csv.DictReader(io.StringIO(text))
pivot = defaultdict(lambda: defaultdict(int))
totals = defaultdict(int)
for row in reader:
    region, month, units = row["region"], row["month"], int(row["units"])
    pivot[region][month] += units
    totals[region] += units
months = sorted({month for values in pivot.values() for month in values})
grand_total = sum(totals.values())
result = {region: {month: pivot[region].get(month, 0) for month in months} for region in sorted(pivot)}
shares = {region: round(100 * totals[region] / grand_total, 1) if grand_total else 0.0 for region in sorted(totals)}
await submit_answer({"pivot": result, "share": shares})
