import csv
import io
from collections import defaultdict
from decimal import Decimal, ROUND_HALF_UP

rows = csv.DictReader(io.StringIO(await read_file({"path": "/data/sales_2026q1.csv"})))
months = ["2026-01", "2026-02", "2026-03"]
pivot = defaultdict(lambda: {month: Decimal("0") for month in months})
for row in rows:
    month = row["date"][:7]
    pivot[row["rep"]][month] += Decimal(row["units"]) * Decimal(row["unit_price"])
result = {
    rep: {month: float(value.quantize(Decimal("0.01"), rounding=ROUND_HALF_UP)) for month, value in monthly.items()}
    for rep, monthly in pivot.items()
}
await submit_answer({"pivot": result})
