# A run of characters with no normalization boundary (a starter and the
# combining marks after it, or Hangul vowels after a leading jamo) is
# normalized in one native step, so micro-monty caps it at 65,536 chars
# after decomposition (docs/sdk.md; past it, MemoryError, where CPython
# has no limit). Runs exactly at the cap normalize as CPython's, in every
# form, among other text and with marks that reorder or decompose.
import unicodedata


def sig(s):
    return len(s), s[:6], s[-6:], sum(map(ord, s[::61]))


runs = [
    'x' + '\u0301' * 65_535,
    'x' + '\u0316\u0301' * 32_767 + '\u0316',
    'e' + '\u0301\u0316' * 32_767 + '\u0323',
    'x' + '\u0344' * 32_767 + '\u0301',
    '\u1100' + '\u1161' * 65_535,
]
for i, run in enumerate(runs):
    s = 'ab\xe9 ' * 3000 + run + ' \u212b'
    for f in ('NFC', 'NFD', 'NFKC', 'NFKD'):
        t = unicodedata.normalize(f, s)
        print(i, f, sig(t), unicodedata.is_normalized(f, s), unicodedata.is_normalized(f, t))
