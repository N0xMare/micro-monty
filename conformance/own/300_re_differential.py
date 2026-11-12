# re: a few hundred generated patterns over generated subjects; every
# result (search spans and groups, findall, sub, split, errors) is printed
# and compared with CPython's.
import re

seed = 12345


def rnd(n):
    global seed
    seed = (seed * 1103515245 + 12345) % 2147483648
    return (seed >> 8) % n


ATOMS = ['a', 'b', 'c', '.', '[ab]', '[^a]', '[a-c]', r'\d', r'\w', r'\s', r'\W', ' ', '1',
         r'\b', r'\B', '^', '$', 'A', '[A-Z]', r'\.', 'x', '\n']
QUANTS = ['', '', '', '*', '+', '?', '*?', '+?', '??', '{2}', '{1,2}', '{0,1}?', '*+', '++', '{2,}']


def gen(depth, ngroups):
    parts = []
    for _ in range(1 + rnd(3)):
        k = rnd(10)
        if depth < 3 and k == 0:
            inner = gen(depth + 1, ngroups)
            ngroups[0] += 1
            atom = '(' + inner + ')'
        elif depth < 3 and k == 1:
            atom = '(?:' + gen(depth + 1, ngroups) + '|' + gen(depth + 1, ngroups) + ')'
        elif depth < 3 and k == 2:
            kind = ['(?=', '(?!', '(?>', '(?<=', '(?<!'][rnd(5)]
            body = ATOMS[rnd(12)] if kind.startswith('(?<') else gen(depth + 1, ngroups)
            atom = kind + body + ')'
        elif k == 3 and ngroups[0] > 0:
            atom = '\\' + str(1 + rnd(ngroups[0]))
        else:
            atom = ATOMS[rnd(len(ATOMS))]
        if not atom.startswith('(?=') and not atom.startswith('(?!') and not atom.startswith('(?<'):
            atom += QUANTS[rnd(len(QUANTS))]
        parts.append(atom)
    out = ''.join(parts)
    if rnd(6) == 0:
        out += '|' + ATOMS[rnd(len(ATOMS))]
    return out


def subject():
    chars = 'aabc 1_A.\nx'
    return ''.join([chars[rnd(len(chars))] for _ in range(rnd(12))])


FLAGS = [0, 0, 0, re.I, re.M, re.S, re.A, re.I | re.M]

for n in range(400):
    pat = gen(0, [0])
    flags = FLAGS[rnd(len(FLAGS))]
    try:
        p = re.compile(pat, flags)
    except (re.error, OverflowError) as e:
        print(n, repr(pat), 'error', type(e).__name__, e)
        continue
    for _ in range(3):
        s = subject()
        m = p.search(s)
        res = [n, repr(pat), flags, repr(s)]
        if m:
            res.append((m.span(), m.groups(), m.lastindex))
        else:
            res.append(None)
        res.append(p.findall(s))
        res.append(p.sub('<\\g<0>>', s))
        res.append(p.split(s))
        res.append(p.match(s) is not None)
        res.append(p.fullmatch(s) is not None)
        print(*res)

# Error messages and positions.
for bad in ['(', ')', '[a', 'a**', '*', '(?P<1>a)', '(?P=x)', r'\1', '(a)\\2', 'a{3,2}',
            '(?<=a+)b', '(?z)', '(?i', r'\x1', r'\u12', '[z-a]', '(?P<a>x)(?P<a>y)',
            '(?#abc', 'a(?i)b', '(?-i)', '(?(2)a|b)', '(?(a)b)', r'\N{DASH}', r'\q',
            '(?<x', 'x\n(', 'a{1,2}{3}', '(?P', '[]', r'[\d-z]', '(?(1)a|b|c)']:
    try:
        re.compile(bad)
        print('ok', repr(bad))
    except re.error as e:
        print(repr(bad), e.msg, e.pos, e.lineno, e.colno, str(e))

# Replacement templates.
for t in [r'\1\1', r'\g<1>-\g<0>', r'\n\t', r'\x', r'\g<2>', r'\g<a>', '\\', r'\400', r'\8']:
    try:
        print(repr(t), re.sub(r'(\w)', t, 'ab'))
    except (re.error, IndexError) as e:
        print(repr(t), type(e).__name__, e)

print(re.sub(r'(?P<w>\w+)', lambda m: m['w'].upper(), 'hello big world', count=2))
print(re.subn('a', 'b', 'aaa'), re.split(r'\b', 'a b'), re.split('x*', 'axbc'))
print(re.findall(r'\w+', 'héllo wörld ßtraße'), re.findall(r'\d', '١٢3'))
print(re.search('STRASSE', 'straße', re.I), re.search('K', '\u212a', re.I))
print(re.match(r'(a)(b)?', 'a').lastindex, re.match(r'(?P<x>a)(?P<y>b)', 'ab').lastgroup)
m = re.search(r'(\d+)-(\d+)', 'tel 123-456 end')
print(m.expand(r'\2/\1'), m.regs, m.pos, m.endpos, m.re.pattern)
print(re.compile(r'(?i)a').flags, re.compile(b'a').flags, re.compile('a', re.A).flags)
print(re.compile('x').search('axbx', 2), re.compile('x').match('axbx', 1, 2))
print(re.compile('^x').search('ax', 1), re.compile('x$').search('xa', 0, 1))
print(re.compile('').findall('ab'), re.compile('a|').findall('ab'))
print(re.escape('a-b.c*'), re.escape(b'a b'))
print([m.span() for m in re.finditer(r'\b', 'ab cd')])
sc = re.compile(r'\d').scanner('a1b2')
print(sc.search(), sc.search(), sc.search())
