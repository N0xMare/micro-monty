import csv
import io

text = await read_file({"path": "/data/sales_2026q1.csv"})
months = ("2026-01", "2026-02", "2026-03")
pivot = {}
for row in csv.DictReader(io.StringIO(text)):
    region = row["region"]
    month = (row.get("month") or row["date"])[:7]
    pivot.setdefault(region, {month: 0 for month in months})[month] += int(row["units"])
total = sum(sum(values.values()) for values in pivot.values())
share = {region: round(100 * sum(values.values()) / total, 1) if total else 0.0 for region, values in pivot.items()}
await submit_answer({"pivot": pivot, "share": share})
