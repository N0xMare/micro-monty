import csv, io
from collections import defaultdict
from datetime import datetime
from decimal import Decimal, ROUND_HALF_UP

text = await read_file({"path": "/data/bank_feb.csv"})
rows = csv.DictReader(io.StringIO(text), delimiter=";")
totals = defaultdict(Decimal)
largest = None
for row in rows:
    amount = Decimal(row["Betrag"].replace(".", "").replace(",", "."))
    purpose = row["Verwendungszweck"]
    totals[purpose] += amount
    if amount < 0 and (largest is None or amount < largest[0]):
        largest = (amount, datetime.strptime(row["Datum"], "%d.%m.%Y").date().isoformat())
await submit_answer({"totals": {key: float(value.quantize(Decimal("0.01"), rounding=ROUND_HALF_UP)) for key, value in totals.items()},
                     "largest_debit_date": largest[1]})
