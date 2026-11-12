import csv
import io
from collections import defaultdict
from datetime import datetime

content = await read_file({"path": "/data/bank_feb.csv"})
rows = list(csv.DictReader(io.StringIO(content), delimiter=";"))
totals = defaultdict(float)
parsed = []
for row in rows:
    amount_text = row["Betrag"].strip().replace(".", "").replace(",", ".")
    amount = float(amount_text)
    totals[row["Verwendungszweck"]] += amount
    if amount < 0:
        date_text = next(value for key, value in row.items() if "datum" in key.lower() or "date" in key.lower() or "buchungstag" in key.lower())
        parsed.append((amount, datetime.strptime(date_text.strip(), "%d.%m.%Y").date().isoformat()))
largest_debit_date = min(parsed, key=lambda item: item[0])[1]
await submit_answer({"totals": {purpose: round(amount, 2) for purpose, amount in totals.items()}, "largest_debit_date": largest_debit_date})
