import csv
import io
from collections import defaultdict

text = await read_file({"path": "/data/sales_2026q1.csv"})
rows = list(csv.DictReader(io.StringIO(text)))
months = ["2026-01", "2026-02", "2026-03"]
pivot = defaultdict(lambda: {m: 0 for m in months})
totals = defaultdict(int)
for row in rows:
    region = row["region"]
    month = row["date"][:7]
    units = int(row["units"])
    if month in months:
        pivot[region][month] += units
        totals[region] += units
grand_total = sum(totals.values())
await submit_answer({
    "pivot": {r: pivot[r] for r in sorted(pivot)},
    "share": {r: round(100 * n / grand_total, 1) for r, n in sorted(totals.items())},
})
