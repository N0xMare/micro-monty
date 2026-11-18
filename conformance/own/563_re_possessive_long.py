# re: a captured group under a possessive repeat, or a greedy one that ends
# its atomic group or lookahead, over 1.6M chars at the default memory
# limit (each iteration drops its frames); parks land inside each match.
import re

s = 'ab' * 800_000
for p, f in [
    ('(a|b)*+', re.fullmatch), ('(?>(a|b)*)', re.fullmatch), ('(?>(?>(?>(a|b)*)))', re.fullmatch),
    ('(a|b)*+c', re.match), ('(?>(a|b)*)c', re.match), ('(?:(a)|(b))*+', re.fullmatch),
    ('(?=(a|b)*)', re.match), ('(?>x?((?:a|b)*))', re.match), ('(?:(a)|(b)){2,}+', re.fullmatch),
]:
    m = f(p, s)
    spans = m and [m.span(g) for g in range(1, len(m.groups()) + 1)]
    print(p, m and (m.span(), spans, [g and g[-2:] for g in m.groups()], m.lastindex))
