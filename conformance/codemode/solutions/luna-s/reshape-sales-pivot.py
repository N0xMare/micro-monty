import csv
import io
from collections import defaultdict
from decimal import Decimal, ROUND_HALF_UP
text = await read_file({"path": "/data/sales_2026q1.csv"})
totals = defaultdict(lambda: defaultdict(Decimal))
for row in csv.DictReader(io.StringIO(text)):
    totals[row["rep"]][row["date"][:7]] += Decimal(row["units"]) * Decimal(row["unit_price"])
pivot = {rep: {month: float(months[month].quantize(Decimal("0.01"), rounding=ROUND_HALF_UP)) for month in ("2026-01", "2026-02", "2026-03")} for rep, months in totals.items()}
await submit_answer({"pivot": pivot})
