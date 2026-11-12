# `str.format` with a short template and a long argument, a wide spec or a
# long output goes to the prelude twin, whose pieces and join go a step at
# a time; an empty spec gives a long str itself. Results, errors and the
# numbering rules must not move.

S = 'ab' * 400_000
E = 'é' * 600_000
t = '{}{}'.format(S, S)
print(len(t), t[:5], t[-5:], t == S + S)
t = '<{0}|{1}|{0}>'.format(S, E)
print(len(t), t[:4], t[799_999:800_004], t[-4:])
r = '{!r}'.format(S)
print(len(r), r[:4], r[-3:])
print(len('{!s}{}'.format(E, 1)), '{}x'.format(S)[-3:])
print('{:>900000}'.format('a')[-3:], len('{:^900000}'.format('a')))
print(len('{:-<700000}|{:.3}'.format('q', S)), '{:.3}'.format(S))
print('{a}{b}'.format(a=S, b='!')[-2:], '{0[1]}'.format([0, S])[:3])
print(format(S, '') == S, len(f'{S}{E}'))
for bad in ('{}{}', '{0}{}', '{2}', '{x}', '{:d}'):
    try:
        print('{}', bad.format(S))
    except (IndexError, KeyError, ValueError) as e:
        print(type(e).__name__, e)


class F:
    def __format__(self, spec):
        return 'F' + spec


print('{}{:wide}'.format(S, F())[-5:])
