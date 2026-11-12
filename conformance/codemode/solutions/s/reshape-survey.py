import csv, io

text = await read_file({"path": "/data/survey.csv"})
reader = csv.DictReader(io.StringIO(text.lstrip("﻿"), newline=""))
rows = [{(k or "").strip().lower(): v for k, v in r.items()} for r in reader]
rows = [r for r in rows if (r.get("respondent") or "").strip()]

scores = []
with_comment = 0
multiline = []
for r in rows:
    s = (r.get("score") or "").strip()
    if s:
        scores.append(float(s))
    c = r.get("comment") or ""
    if c.strip():
        with_comment += 1
    if "\n" in c.strip() or "\r" in c.strip():
        rid = r["respondent"].strip()
        multiline.append(int(rid) if rid.isdigit() else rid)

multiline.sort()
answer = {
    "responses": len(rows),
    "average_score": round(sum(scores) / len(scores), 2) if scores else 0,
    "with_comment": with_comment,
    "multiline": multiline,
}
print(answer, "scores counted:", len(scores))
print(await submit_answer(answer))
