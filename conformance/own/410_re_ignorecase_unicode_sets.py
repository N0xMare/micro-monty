# re.IGNORECASE character sets fold through both cases of the subject
# character and compare a set literal by its lowercase: the Kelvin sign
# matches [J-M], [19K] and [19k]; long s matches [a-z] and [r-t].
import re
for p, s in [('[J-M]', '\u212a'), ('[j-m]', '\u212a'), ('[19K]', '\u212a'),
             ('[19k]', '\u212a'), ('[19\u212a]', 'K'), ('[19\u212a]', 'k'),
             ('\u0412', '\u1c80'), ('\u1c80', '\u0432'), ('k', '\u212a'),
             ('\u212a', 'k'), ('s', '\u017f'), ('[s]', '\u017f'),
             ('[a-z]', '\u017f'), ('[r-t]', '\u017f'), ('[^a-z]', '\u017f'),
             ('\u00b5', '\u03bc'), ('[\u00b5]', '\u039c'), ('[^k]', 'K'),
             ('[A-Z]+', 'h\u00e9llo'), ('[a-z\u00e0-\u00ff]+', 'H\u00c9LLO'),
             ('[\u03c3]', '\u03c2'), ('[\u03a3]', '\u03c2')]:
    print(ascii(p), ascii(s), re.match(p, s, re.I) is not None,
          re.match(p, s, re.I | re.A) is not None)
print(re.findall(r'(?i)[k-m]+', 'KELVIN \u212a kilo LAMP'))
print(re.sub(r'(?i)[a-z]', '.', 'Stra\u017fe \u212a'))
