import csv, io

text = await read_file({"path": "/data/survey.csv"})
rows = list(csv.DictReader(io.StringIO(text, newline="")))
scores = [float(row["score"]) for row in rows]
with_comment = sum(bool(row["comment"].strip()) for row in rows)
multiline = sorted(int(row["respondent"]) for row in rows if "\n" in row["comment"] or "\r" in row["comment"])
await submit_answer({"responses": len(rows), "average_score": round(sum(scores) / len(scores), 2) if scores else 0.0,
                     "with_comment": with_comment, "multiline": multiline})
