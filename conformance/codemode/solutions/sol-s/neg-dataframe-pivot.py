import csv
import io

text = await read_file({"path": "/data/sales_2026q1.csv"})
rows = list(csv.DictReader(io.StringIO(text)))
pivot = {}
for row in rows:
    month = row.get("month", row.get("date", ""))[:7]
    region = pivot.setdefault(row["region"], {"2026-01": 0, "2026-02": 0, "2026-03": 0})
    region[month] += int(row["units"])
totals = {region: sum(months.values()) for region, months in pivot.items()}
total = sum(totals.values())
share = {region: round(units * 100 / total, 1) if total else 0.0 for region, units in totals.items()}
await submit_answer({"pivot": pivot, "share": share})
