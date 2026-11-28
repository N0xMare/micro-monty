# An int's format spec reports an unknown presentation type before it
# refuses `z`, as CPython's switch on the type does: `format(255, 'zs')`
# raised `Negative zero coercion (z) not allowed in integer format
# specifier` (difffuzz). The same through format(), str.format and an
# f-string; a precision is still refused first, and `z` on a known int
# code or on a float code is unchanged.
for v in (255, -3, 0, 2 ** 70, -(10 ** 25)):
    for spec in ('zs', '>zs', '+zs', ' zs', 'z#s', 'zq', 'zr', 'z,s', 'z.2s', 'z.2d',
                 'z', 'zd', 'zx', 'zc', 'zn', 'z,d', 'z_x', 'z#x', 'z08d',
                 'zf', 'z.1f', 'z%', 'ze', 'zg'):
        out = []
        for how in range(3):
            try:
                if how == 0:
                    r = format(v, spec)
                elif how == 1:
                    r = ('{:' + spec + '}').format(v)
                else:
                    r = f'{v:{spec}}'
                out.append(repr(r))
            except ValueError as e:
                out.append(f'ValueError: {e}')
        print(repr(v)[:12], spec, out[0] if out[0] == out[1] == out[2] else out)
