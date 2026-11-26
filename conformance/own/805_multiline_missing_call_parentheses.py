# The missing-call-parentheses diagnostic spans the entire argument.
from ast import parse

try:
    parse('print 1 + (\n    2 + 3\n)\n')
except SyntaxError as err:
    print(err.msg, (err.lineno, err.offset), (err.end_lineno, err.end_offset))
