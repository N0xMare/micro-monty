# Format-spec edges: precisions past Rust's formatter limit, `z` on values
# that round to zero, a bare `0` width on strings, the order of string-spec
# errors, `!a` and int[...].
print(len(f'{1.5:.70000f}'), len(format(1.5, '#.70000g')), format(2.5, '.70000e')[-6:], format(1e300, '.2000f')[-3:])
print(len('%.70000f' % 1.5), '%.2147482624g' % 1.5, '%.2000000000f' % float('inf'), format(1e-10, '.100000g'))
print(f'{-0.001:z.1f}', f'{-0.0:z}', f'{-1e-9:z.2e}', f'{-0.4:z.0f}', f'{float("-inf"):z}', f'{-0.0:z>8.1f}')
print(f'{123!s:05}', f'{"ab":06}', f'{"café"!a}', ascii('é'), repr('\U000f4240'))
for spec in ('#', ',', '_', '+', ' ', '=', '#x', '.2f', '=#', ',x', 'z', 'z#', ' #'):
    try:
        print(repr(format('x', spec)))
    except ValueError as e:
        print(spec, 'ValueError', e)
for v, spec in ((5, 'z'), (True, 'z'), (5, 'z.2'), (1.5, f'.{2**31}g'), (1 + 2j, f'.{2**31}g'), (1.5, f'.{2**31 - 1024}g')):
    try:
        print(format(v, spec))
    except ValueError as e:
        print(spec[:6], 'ValueError', e)
try:
    int[str]
except TypeError as e:
    print('TypeError', e)

# Zero-fill joins the grouped digits, but nan/inf have no digit groups.
for value in (0, 1234, -1234, -0.01, 1e20, 1e308, float('nan'), float('inf'), float('-inf')):
    for spec in ('+010,', '010,.2f', 'z010,.0f', '#010,', '#010,.0%'):
        print(repr(value), spec, repr(format(value, spec)))
for value in (0, 1234, -1234):
    print(format(value, '#010_x'))
