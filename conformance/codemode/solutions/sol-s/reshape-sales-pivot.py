import csv
import io
from decimal import Decimal

text = await read_file({"path": "/data/sales_2026q1.csv"})
months = ["2026-01", "2026-02", "2026-03"]
pivot = {}
for row in csv.DictReader(io.StringIO(text)):
    rep = pivot.setdefault(row["rep"], dict.fromkeys(months, Decimal(0)))
    rep[row["date"][:7]] += int(row["units"]) * Decimal(row["unit_price"])
await submit_answer({"pivot": {name: {month: float(round(value, 2)) for month, value in values.items()} for name, values in pivot.items()}})
