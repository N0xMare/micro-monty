import csv
import io

rows = list(csv.DictReader(io.StringIO(await read_file({"path": "/data/survey.csv"}), newline="")))
await submit_answer({"responses": len(rows), "average_score": round(sum(float(r["score"]) for r in rows) / len(rows), 2), "with_comment": sum(bool(r["comment"].strip()) for r in rows), "multiline": sorted(int(r["respondent"]) for r in rows if "\n" in r["comment"] or "\r" in r["comment"])})
