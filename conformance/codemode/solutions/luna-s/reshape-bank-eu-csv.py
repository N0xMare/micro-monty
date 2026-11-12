import csv
import io
from datetime import datetime
from decimal import Decimal
text = await read_file({"path": "/data/bank_feb.csv"})
totals = {}
largest = None
for row in csv.DictReader(io.StringIO(text), delimiter=";"):
    amount = Decimal(row["Betrag"].strip().replace(".", "").replace(",", "."))
    purpose = row["Verwendungszweck"]
    totals[purpose] = totals.get(purpose, Decimal("0")) + amount
    if amount < 0 and (largest is None or amount < largest[0]):
        largest = (amount, row["Datum"])
date = datetime.strptime(largest[1], "%d.%m.%Y").strftime("%Y-%m-%d")
await submit_answer({"totals": {key: round(float(value), 2) for key, value in totals.items()}, "largest_debit_date": date})
