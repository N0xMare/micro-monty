# A float's format with a precision and no presentation type is 'g' that
# keeps a digit past the point, so it switches to exponent form at
# exponent precision - 1 ('{:.3}'.format(100.0) is '1e+02', not '100.0').
# 'c' right-aligns like the other integer presentations; '_' or ',' in the
# fraction is refused with 'n'.
values = [0.0, -0.0, 0.5, 1.0, 5.0, 9.99, 12.0, 99.5, 100.0, 123.456, 1234.5, 99999.0,
          1e16, 1.5e-5, 0.00012345, -2.5, 3.14159, 2.675, float('inf'), float('nan')]
for spec in ['.0', '.1', '.2', '.3', '.4', '.6', '.17', '#.3', '10.2', '<10.3', '+.2', ',.3', '_.5', '.3%']:
    print(spec, [format(v, spec) for v in values])
x = 1234.5678
print(f'{x:.2} {x:.5} {x:.6} {x:.7} {x:.8} {x:,.6} {x:z.1}')
print('{:.3} {:.2}'.format(100.0, 12.0), str(1e16), repr(1e-5))
print(round(2.5), f'{2.5:.1}', f'{0.25:.1}', f'{0.35:.1}')
print([f'{65:5c}', f'{65:<5c}', f'{65:^5c}', f'{0x263a:3c}', f'{97:c}', '{:>3c}|{:3c}'.format(98, 99)])
for spec in ['.6_n', '.6,n', '.3_f', '.3_', '._6f', '.2_e']:
    for v in (1.5, 3, 2 + 1j):
        try:
            r = format(v, spec)
        except ValueError as e:
            r = f'ValueError: {e}'
        print(spec, type(v).__name__, r)
print([format(v, s) for v in (0.0, -0.0, 1.0, 5.5) for s in ('#.0','#.1','#.2', '#', '.1', '.0', '.2')])
