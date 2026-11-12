import csv
import io
rows = list(csv.DictReader(io.StringIO(await read_file({"path": "/data/survey.csv"}), newline="")))
scores = [float(row["score"]) for row in rows]
multiline = sorted(int(row["respondent"]) for row in rows if len(row["comment"].splitlines()) > 1)
with_comment = sum(bool(row["comment"].strip()) for row in rows)
await submit_answer({"responses": len(rows), "average_score": round(sum(scores) / len(scores), 2), "with_comment": with_comment, "multiline": multiline})
