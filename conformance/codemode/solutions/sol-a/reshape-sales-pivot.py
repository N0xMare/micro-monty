import csv
import io
from decimal import Decimal

rows = csv.DictReader(io.StringIO(await read_file({"path": "/data/sales_2026q1.csv"})))
pivot = {}
for row in rows:
    months = pivot.setdefault(row["rep"], {m: Decimal(0) for m in ["2026-01", "2026-02", "2026-03"]})
    months[row["date"][:7]] += int(row["units"]) * Decimal(row["unit_price"])
await submit_answer({"pivot": {rep: {m: float(round(value, 2)) for m, value in months.items()} for rep, months in pivot.items()}})
