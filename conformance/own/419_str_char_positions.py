# Char indexes into non-ASCII strings, found from the nearest remembered
# position: every index from both ends, cursors far apart, slices with
# steps, method windows, re positions and json offsets, over 1- to 4-byte
# chars and lengths around the 8-byte steps of a walk.
import json
import re

ALPHABET = 'a\x7f\x80é\u07ff\u0800€\uffff\U00010000😀\U0010ffff'


def text(n, seed):
    out = []
    for _ in range(n):
        seed = (seed * 1103515245 + 12345) % 2147483648
        out.append(ALPHABET[seed % len(ALPHABET)])
    return ''.join(out)


def digest(xs):
    h = 0
    for x in xs:
        for c in str(x):
            h = (h * 31 + ord(c)) % 1000000007
    return h


for n in (1, 7, 8, 9, 15, 16, 17, 64, 65, 300):
    s = text(n, n)
    chars = list(s)
    fwd = [s[i] for i in range(n)]
    back = [s[i] for i in range(n - 1, -1, -1)]
    neg = [s[-1 - i] for i in range(n)]
    ends = [s[i] + s[n - 1 - i] for i in range(n)]
    assert fwd == chars and back == chars[::-1] and neg == chars[::-1], n
    assert ends == [chars[i] + chars[n - 1 - i] for i in range(n)], n
    jumps = []
    seed = 1
    for _ in range(3 * n):
        seed = (seed * 69069 + 1) % 4294967296
        jumps.append(s[seed % (2 * n) - n])
    slices = [s[a:b:c] for a in (None, 0, 3, -5, n // 2, n + 2)
              for b in (None, 1, -2, n - 1, n + 5)
              for c in (None, 1, 2, 7, -1, -3, n + 1)]
    print(n, len(s.encode()), s[0], s[-1], digest(jumps), digest(slices))
    for x in ('€', 'é€', s[n // 2], s[n // 3:n // 3 + 2], s[1:12]):
        print(' ', s.find(x), s.rfind(x), s.find(x, n // 4), s.rfind(x, 0, n // 2),
              s.count(x, 2, -2), s.startswith(x, n // 3), s.endswith(x, 0, n // 3 + 2))

# Two cursors far apart in one long string, past collections.
long = text(5000, 42) * 4
junk = []
picked = []
for i in range(0, 10000, 7):
    junk.append(text(20, i))
    picked.append(long[i] + long[-1 - i] + long[i:i + 3])
print(digest(picked), len(junk))

# A cursor through a string: find, startswith and re positions.
src = 'naïve = "café" # ünï\n' * 30 + 'end = 1\n'
i = lines = comments = 0
while i < len(src):
    j = src.find('\n', i)
    if src.startswith('naïve', i):
        lines += 1
    k = src.find('#', i, j)
    if k >= 0:
        comments += 1
    i = j + 1
print(lines, comments, src.rfind('ü'), src.index('end'))
tok = re.compile(r'\s*(?:([^\W\d]\w*)|(\d+)|("[^"]*")|(#[^\n]*)|(.)|\Z)')
pos = 0
kinds = []
while pos < len(src):
    m = tok.match(src, pos)
    kinds.append(m.lastindex)
    pos = m.end()
print(len(kinds), digest(kinds), [m.span() for m in re.compile('é|ï').finditer(src, 30, 80)])

# json offsets are char offsets.
docs = '["é😀", {"ü": 1}]  "€"  [2, "ß"]'
dec = json.JSONDecoder()
i = 0
while i < len(docs):
    while docs[i] == ' ':
        i += 1
    v, i = dec.raw_decode(docs, i)
    print(v, i)
for bad in ('["é😀", 1,]', '{"ü": tru}', '"€\x01"', '[1] €'):
    try:
        json.loads(bad)
    except json.JSONDecodeError as e:
        print(e.msg, e.pos, e.colno)
