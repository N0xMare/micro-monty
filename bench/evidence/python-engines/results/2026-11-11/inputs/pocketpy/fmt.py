# fmt: format 60k records with f-strings (padding, hex, width), `%`
# formatting and `str.join`, then join them into one report.
lines = []
for i in range(60000):
    name = f'user{i:06d}'
    head = f'{name}|{i % 97:>4}|{i * 31 % 4096:x}'
    tail = '%s:%d:%-5s' % (name[-3:], i % 13, 'ok' if i % 2 else 'no')
    lines.append('|'.join([head, tail, str(i * 7)]))
report = '\n'.join(lines)
print(len(report), report.count('ok'), report.count('|  '), lines[12345])
