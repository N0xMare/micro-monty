import csv
import io

text = await read_file({"path": "/data/survey.csv"})
rows = list(csv.DictReader(io.StringIO(text, newline="")))


def rid(v):
    v = v.strip()
    return int(v) if v.lstrip("-").isdigit() else v


scores = []
with_comment = 0
multiline = []
for r in rows:
    scores.append(float(r["score"]))
    comment = r.get("comment") or ""
    if comment.strip():
        with_comment += 1
    if "\n" in comment.strip() or "\r" in comment.strip():
        multiline.append(rid(r["respondent"]))

answer = {
    "responses": len(rows),
    "average_score": round(sum(scores) / len(scores), 2) if scores else 0.0,
    "with_comment": with_comment,
    "multiline": sorted(multiline),
}
print(answer)
print(await submit_answer(answer))
