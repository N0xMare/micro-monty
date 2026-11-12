import csv, io
from decimal import Decimal

rows = csv.DictReader(io.StringIO(await read_file({"path": "/data/sales_2026q1.csv"})))
pivot = {}
for r in rows:
    months = pivot.setdefault(r["rep"], {m: Decimal(0) for m in ["2026-01", "2026-02", "2026-03"]})
    months[r["date"][:7]] += int(r["units"]) * Decimal(r["unit_price"])
await submit_answer({"pivot": {rep: {m: float(round(v, 2)) for m, v in months.items()} for rep, months in pivot.items()}})
