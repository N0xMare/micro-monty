# A `str.format` field index of 4294967295 (u32::MAX, the largest the
# native path parses) is out of range like any other, also beside other
# fields, auto-numbered ones, conversions and specs: it was taken for the
# receiver, argument 0 (the `run_source` fuzz target's crash-8702f1e5).
for t in ('{4294967295}', '{4294967295!r}', '{4294967295:>3}', '{0}{4294967295}',
          '{4294967294}', 'a{4294967294}b', '{4294967296}', '{0}', '{1}',
          '{}', '{}{}', '{}{4294967295}', '{4294967295}{}', '{}{4294967294}'):
    for args in ((), ('x',), ('x', 'y')):
        try:
            print(repr(t), len(args), repr(t.format(*args)))
        except (IndexError, ValueError) as e:
            print(repr(t), len(args), type(e).__name__, e)
