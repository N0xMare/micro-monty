import csv, io
from decimal import Decimal
from datetime import datetime
from collections import defaultdict

rows = list(csv.DictReader(io.StringIO(await read_file({"path": "/data/bank_feb.csv"})), delimiter=";"))
totals = defaultdict(Decimal)
debits = []
for row in rows:
    amount = Decimal(row["Betrag"].replace(".", "").replace(",", "."))
    totals[row["Verwendungszweck"]] += amount
    if amount < 0:
        debits.append((amount, row["Datum"]))
date = datetime.strptime(min(debits, key=lambda d: d[0])[1], "%d.%m.%Y").date().isoformat()
await submit_answer({"totals": {k: float(round(v, 2)) for k, v in totals.items()}, "largest_debit_date": date})
