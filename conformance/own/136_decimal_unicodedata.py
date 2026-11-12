# decimal and unicodedata (optional modules; mm-cli enables both).
import unicodedata
from decimal import ROUND_HALF_UP, Decimal, DivisionByZero, InvalidOperation, getcontext, localcontext

a, b = Decimal('1.10'), Decimal('2.205')
print(a + b, a * b, b / a, b // a, b % a, -a, abs(-b), a ** 3, b.sqrt())
print(b.quantize(Decimal('0.01')), b.quantize(Decimal('0.01'), rounding=ROUND_HALF_UP), round(b, 1), int(b))
print(repr(Decimal(0.1)), Decimal('1e3').normalize(), Decimal('-0'), Decimal('NaN').is_nan(), a < b, a == 1.1)
print(f'{b:>10.2f}|{Decimal("1234567.891"):,.2f}|{a:%}|{Decimal("1e-7"):e}', format(Decimal('12.5'), 'g'))
with localcontext(prec=5) as ctx:
    print(Decimal(1) / 7, ctx.prec, getcontext().prec)
print(getcontext().prec, sum(Decimal('0.1') for _ in range(10)))
for bad in (lambda: Decimal(1) / 0, lambda: Decimal('x'), lambda: Decimal(0) / 0):
    try:
        bad()
    except (DivisionByZero, InvalidOperation) as e:
        print(type(e).__name__, isinstance(e, ArithmeticError))

for c in 'aÅ٣½€\u0301한𝟘':
    print(repr(c), unicodedata.category(c), unicodedata.name(c, '?'), unicodedata.numeric(c, None),
          unicodedata.combining(c), unicodedata.decomposition(c))
s = 'Ame\u0301lie ﬁ Å'
for form in ('NFC', 'NFD', 'NFKC', 'NFKD'):
    n = unicodedata.normalize(form, s)
    print(form, ascii(n), len(n), unicodedata.is_normalized(form, n))
print(unicodedata.lookup('GREEK SMALL LETTER ALPHA'), unicodedata.lookup('hangul syllable gag'), unicodedata.unidata_version)
try:
    unicodedata.lookup('NO SUCH CHARACTER')
except KeyError as e:
    print('KeyError', e)
