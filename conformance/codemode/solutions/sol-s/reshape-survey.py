import csv
import io
from decimal import Decimal

text = await read_file({"path": "/data/survey.csv"})
rows = list(csv.DictReader(io.StringIO(text, newline="")))
await submit_answer({
    "responses": len(rows),
    "average_score": float(round(sum(Decimal(r["score"]) for r in rows) / len(rows), 2)),
    "with_comment": sum(bool(r["comment"].strip()) for r in rows),
    "multiline": sorted(int(r["respondent"]) for r in rows if "\n" in r["comment"] or "\r" in r["comment"]),
})
