# tokenize reports doubled braces as literal pieces, not replacement fields.
from io import StringIO
from tokenize import generate_tokens, tok_name

for prefix in ('f', 't'):
    source = prefix + "'left{{mid}}:{v!r:>5}end'"
    print([(tok_name[t.type], t.string, t.start, t.end)
           for t in generate_tokens(StringIO(source).readline)])
