import csv, io
from decimal import Decimal, ROUND_HALF_UP

text = await read_file({"path": "/data/sales_2026q1.csv"})
rows = list(csv.DictReader(io.StringIO(text)))
months = ["2026-01", "2026-02", "2026-03"]

pivot = {}
skipped = []
for r in rows:
    r = {(k or "").strip(): (v or "").strip() for k, v in r.items()}
    try:
        month = r["date"][:7]
        rev = Decimal(r["units"]) * Decimal(r["unit_price"])
    except Exception:
        skipped.append(r)
        continue
    rep = r["rep"]
    pivot.setdefault(rep, {m: Decimal(0) for m in months})
    if month not in pivot[rep]:
        skipped.append(r)
        continue
    pivot[rep][month] += rev

result = {
    rep: {m: float(v.quantize(Decimal("0.01"), rounding=ROUND_HALF_UP)) for m, v in sorted(ms.items())}
    for rep, ms in sorted(pivot.items())
}
print("skipped:", skipped)
for rep, ms in result.items():
    print(rep, ms)
print(await submit_answer({"pivot": result}))
