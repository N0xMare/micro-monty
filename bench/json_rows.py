# json: serialise 50k rows to text, then parse them back.
import json

rows = []
for i in range(50_000):
    rows.append({'id': i, 'name': 'user' + str(i), 'active': i % 2 == 0, 'tags': ['a', 'b'], 'score': i * 3})
text = json.dumps(rows, separators=(',', ':'))
back = json.loads(text)
total = 0
for row in back:
    total += row['score']
print(len(text), len(back), total)
