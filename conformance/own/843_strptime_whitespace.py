from time import strptime
import warnings

# A run can cross several native windows in either operand. Keep the
# original format and data offsets, including adjacent directives that
# need to backtrack after a greedy numeric match.
pad = ' \t\v\x1c' * 70_000
assert strptime('a' + pad + '1112024', 'A %m%d%Y')[:3] == (2024, 11, 1)
assert strptime('a\t2024', 'A' + pad + '%Y').tm_year == 2024
assert strptime(pad + '2024' + pad, pad + '%Y' + pad).tm_year == 2024

# Non-ASCII, unsupported directives, and native misses still use the
# original Python path, including its warnings and error text.
assert strptime('\u00a0\u0662\u0660\u0662\u0664', ' %Y').tm_year == 2024
assert strptime('2024 01 1', '%G %V %u')[:3] == (2024, 1, 1)
for data, fmt, message in [
    ('123', '%H', 'unconverted data remains: 3'),
    ('x', '%Y', "time data 'x' does not match format '%Y'"),
]:
    try:
        strptime(data, fmt)
    except ValueError as exc:
        assert str(exc) == message
    else:
        raise AssertionError('expected ValueError')

with warnings.catch_warnings():
    warnings.simplefilter('error', DeprecationWarning)
    try:
        strptime('03-14', '%m-%d')
    except DeprecationWarning:
        pass
    else:
        raise AssertionError('expected DeprecationWarning')
