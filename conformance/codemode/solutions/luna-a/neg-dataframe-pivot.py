import csv
from collections import defaultdict
from datetime import datetime

text = await read_file({"path": "/data/sales_2026q1.csv"})
rows = list(csv.DictReader(text.splitlines()))
months = sorted({datetime.strptime(row["date"], "%Y-%m-%d").strftime("%Y-%m") for row in rows})
regions = sorted({row["region"] for row in rows})
totals = defaultdict(int)
region_totals = defaultdict(int)
for row in rows:
    month = datetime.strptime(row["date"], "%Y-%m-%d").strftime("%Y-%m")
    units = int(row["units"])
    totals[(row["region"], month)] += units
    region_totals[row["region"]] += units
all_units = sum(region_totals.values())
pivot = {region: {month: totals[(region, month)] for month in months} for region in regions}
share = {region: round(100 * region_totals[region] / all_units, 1) for region in regions}
await submit_answer({"pivot": pivot, "share": share})
