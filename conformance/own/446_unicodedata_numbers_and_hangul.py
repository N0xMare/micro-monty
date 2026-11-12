# unicodedata.decimal/digit with and without defaults, Hangul syllable
# decomposition and composition, algorithmic names both ways.
import unicodedata as u
print(u.decimal('7'), u.digit('9'), u.decimal('x', None), u.digit('²'), u.numeric('½'))
for f in (u.decimal, u.digit, u.numeric):
    try:
        f('x')
    except ValueError as e:
        print(e)
print(u.decimal('a', -1), u.digit('a', 'd'), u.numeric('a', 0.5))
print(u.normalize('NFD', '한'), [hex(ord(c)) for c in u.normalize('NFD', '한글')], u.normalize('NFC', u.normalize('NFD', '한글')) == '한글')
print([hex(ord(c)) for c in u.normalize('NFD', 'ệ')], u.normalize('NFKD', 'ﬁ'), len(u.normalize('NFD', 'a\u0328\u0301')))
print(u.lookup('CJK UNIFIED IDEOGRAPH-4E00'), u.lookup('HANGUL SYLLABLE GA'), u.name('\u4e00'), u.name('가'))
try:
    u.lookup('CJK UNIFIED IDEOGRAPH-4e00')
except KeyError as e:
    print('KeyError', e)
print(u.decomposition('가'), u.decomposition('각'))
