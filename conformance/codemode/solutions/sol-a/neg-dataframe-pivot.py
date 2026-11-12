import csv
import io

text = await read_file({"path": "/data/sales_2026q1.csv"})
pivot = {}
for row in csv.DictReader(io.StringIO(text)):
    month = (row.get("month") or row["date"])[:7]
    region = pivot.setdefault(row["region"], {"2026-01": 0, "2026-02": 0, "2026-03": 0})
    region[month] += int(row["units"])
total = sum(sum(months.values()) for months in pivot.values())
share = {region: round(100 * sum(months.values()) / total, 1) if total else 0.0 for region, months in pivot.items()}
await submit_answer({"pivot": pivot, "share": share})
