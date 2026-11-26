# A str format spec with an explicit `0=` fill and alignment followed by
# the `0` flag (`0=0.6`, `0=05`) raises CPython's "'=' alignment not
# allowed", short and long; the implicit `=` of a bare `0` (`05`, `010`)
# pads with zeros, and numbers keep their padding.


def fmt(v, spec):
    try:
        return format(v, spec)
    except ValueError as e:
        return str(e)


refused = "'=' alignment not allowed in string format specifier"
for s in ('été', 'é' * 300_000):
    assert fmt(s, '0=0.6') == refused and fmt(s, '0=05') == refused
    assert fmt(s, '0=5') == refused and fmt(s, 'x=05') == refused
assert fmt('été', '05') == 'été00' and fmt('été', '010') == 'été0000000'
assert fmt('ab', '0>5') == '000ab' and fmt('ab', '0<05') == 'ab000'
assert fmt('été', '0.2') == 'ét' and fmt('é' * 300_000, '05') == 'é' * 300_000
assert fmt(7, '0=05') == '00007' and fmt(-42, '0=06') == '-00042'
assert fmt(-1.25, '0=08') == '-0001.25' and fmt(2.5, 'x=05') == 'xx2.5'
