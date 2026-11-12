# Bounded native steps for unicodedata (docs/spec.md §5): a long str is
# normalized a slice a step, each slice ending before a char that composes
# with nothing before it, and the result joined a slice a step. Results
# must equal one native run's, and CPython's.
import unicodedata


def sig(s):
    return len(s), s[:12], s[-12:], sum(map(ord, s[::97]))


pool = ['a', 'e', ' ', '\u0301', '\u0308', '\u0323', '\u0302', '\u0345', '\u0344', '\u0f71\u0f72',
        '\u1100', '\u1161', '\u11a8', '\uac00', '\uac01', '\u0b47', '\u0b3e', '\u0b57', '\u0bc6', '\u0bbe',
        '\ufb01', '\ufdfa', '\u2126', '\u212b', '\xe9', '\u1e9b', '\u0385', '\u3099', '\u304b', '\u30ab',
        '\u0436', '\u0418', '\u0306', '\u05b4', '\u0591', '\U0001d15e', '\U0001d165', '\u0915', '\u093c',
        '\u0958', '\u2460', '\uff21', '\u0f69']
seed = 7


def make(n, p):
    global seed
    out = []
    for _ in range(n):
        seed = (seed * 1103515245 + 12345) % 2147483648
        out.append(p[seed % len(p)])
    return ''.join(out)


cases = [make(30_000, pool), make(20_000, pool[3:10]), make(20_000, pool[10:20]),
         'e\u0301' * 40_000, '\xe9' * 60_000, '\uac00' * 30_000 + '\u11a8', '\u0b47' + '\u0b3e' * 10_000,
         'x' + '\u0301\u0323' * 6_000, 'abc' * 30_000 + '\u2126']
for i, s in enumerate(cases):
    for form in ('NFC', 'NFD', 'NFKC', 'NFKD'):
        r = unicodedata.normalize(form, s)
        print(i, form, sig(r), r == s, unicodedata.is_normalized(form, s))
