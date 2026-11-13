# `Σ` lowers to final `ς` by the classes of Unicode 16, CPython 3.14's:
# U+0295 is cased there (Ll), and code points unassigned there are
# neither cased nor case-ignorable, though later tables make some of them
# so. Short strings and the stepped path past 512K chars agree.
RANGES = (
    (0x295, 0x295), (0x558, 0x558), (0x58B, 0x58C), (0x5C8, 0x5C9),
    (0xB53, 0xB54), (0x1ACF, 0x1AF0), (0x208F, 0x209F), (0xA7CE, 0xA7D5),
    (0xA7DD, 0xA7DD), (0xA7E2, 0xA7E2), (0xA7F1, 0xA7F1), (0xAB6C, 0xAB6D),
    (0x107BB, 0x107BF), (0x10EC5, 0x10ECF), (0x10EF0, 0x10EFB),
    (0x11B60, 0x11B66), (0x11DD9, 0x11DD9), (0x11DF0, 0x11DF0),
    (0x16EA0, 0x16ED3), (0x16FF2, 0x16FF3), (0x1D127, 0x1D128),
    (0x1D25B, 0x1D25C), (0x1D6A6, 0x1D6A6), (0x1DF1F, 0x1DF2B),
    (0x1DF7F, 0x1DF96), (0x1DFCD, 0x1DFFF), (0x1E6E3, 0x1E6FF),
)
# Assigned neighbours: cased letters, combining marks, a modifier letter,
# apostrophes, a format char, a digit, a space.
OTHERS = 'aZǅªⒶ\u0301\u0345\u02b0\'.:\u00ad\u200d5 ʔʖ'
cps = [chr(u) for a, b in RANGES for u in range(a, b + 1)] + list(OTHERS)

for c in cps:
    rs = (
        (c + 'Σ').lower(), ('A' + c + 'Σ').lower(), ('AΣ' + c).lower(),
        ('AΣ' + c + 'b').lower(), (c + 'Σ').swapcase(), ('a' + c + 'Σ').swapcase(),
        ('xΣ' + c + 'Σ').capitalize(), ('A' + c + 'Σ').title(), ('ΣΣ' + c).lower(),
        (c + 'ΣA').lower(), ('A' + c + c + 'Σ' + c).lower(),
    )
    print(f'{ord(c):x}', ''.join('1' if r.endswith('ς') or 'ς' in r else '0' for r in rs))

unit = ''.join('A' + c + 'Σ ' + c + 'Σ' + 'x' + 'Σ' + c + ' ' for c in cps)
s = unit * (600_000 // len(unit) + 1)
print(len(s) > 524_288)
for low in (s.lower(), s.swapcase(), s.title(), s.capitalize()):
    at = [i for i, ch in enumerate(low[:len(unit)]) if ch == 'ς']
    print(low.count('ς'), len(at), at[:8], at[-8:])
