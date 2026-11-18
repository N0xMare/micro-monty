# re: each iteration of a possessive repeat is atomic, the first `min`
# included; an atomic group is atomic only as a whole. Captured groups keep
# their last iteration's value, and a failed tail leaves them unset.
import re

cases = [
    (r'(?:a|ab){2}+', 'aba'), (r'(?>(?:a|ab){2})', 'aba'), (r'(?:a|ab){2,}+', 'aba'),
    (r'(?>(?:a|ab){2,})', 'aba'), (r'(?:a|ab){1,}+b', 'abb'), (r'(?:a|ab){2}+b', 'aab'),
    (r'(?:ab|a){2}+b', 'abab'), (r'(?:a|ab){0,2}+b', 'ab'), (r'(?:a|ab)*+b', 'aab'),
    (r'(?:(a)|(ab)){2}+', 'aba'), (r'(?:(a)|(ab)){1,3}+', 'aba'), (r'(?:(a)|(ab)){1,3}+c', 'abac'),
    (r'(?>(?:ab|a)*)b', 'aab'), (r'(?>(?:(a)|(ab))*)b', 'aab'),
    (r'(?:a?){3}+', 'a'), (r'(?:a?){3,5}+b', 'ab'), (r'(?:()|a)*+', 'aa'), (r'(a|())*+', 'aa'),
    (r'(?>(a|())*)', 'aa'), (r'(?:(a)|b)*+(?(1)x|y)', 'bab'),
    (r'(a|b)*+c', 'abab'), (r'(a|b)*+c', 'ababc'), (r'(?>(a|b)*)c', 'abab'),
    (r'(?:(a)|(b))*+', 'abab'), (r'(a|b)*+', 'abab'), (r'(?>(a|b)*)', 'abab'),
    (r'(?>(?>(?>(a|b)*)))', 'abab'), (r'(?>x(a|b)*)', 'xabab'), (r'(?>x(a|b)*)y', 'xababy'),
    (r'(?>((a|b)*))', 'abab'), (r'(?>(a|b)*a)', 'abab'), (r'(?=(a|b)*)', 'abab'),
    (r'(?=((a|b)*))ab', 'abab'), (r'(?!(a|b)*c)', 'abab'), (r'(?!(a|b)*)', 'ab'),
    (r'(?:(a)|b|c()){0,3}+', 'abca'), (r'(?>(a|b)*c|(a|b)*)', 'abab'),
    (r'(?:(?:(a)|b)*+)+', 'ab'), (r'((?:a|b)*+)(c)?', 'abc'), (r'(?:(a)|b){2,}+', 'ba'),
    (r'(?:(a)|b){2,}+', 'b'), (r'(?>(?:(a)|(b))*?)c', 'abc'), (r'(?:(x)|(a)b)*+a', 'ababa'),
    (r'x(?:(a)|ab)*+bc', 'xabbc'), (r'x(?:(a)|b)*+(?<=b)', 'xab'),
    (rb'(?:a|ab){2}+', b'aba'), (rb'(?>(a|b)*)c', b'ababc'),
]
for p, s in cases:
    for f in (re.match, re.search, re.fullmatch):
        m = f(p, s)
        print(f.__name__, p, s, m and (m.span(), m.groups(), m.lastindex))
print(re.findall(r'(?:a|ab){2}+', 'aaabaab'), re.sub(r'(?:(a)|b)*+', r'<\1>', 'abxba'))
# Around the 64th iteration, where a repeat past `min` folds its stack, and
# with many groups set under the repeat.
for n in (63, 64, 65, 127, 128, 129, 200):
    for p, s in [
        (r'(?:(a)|(b)){64,}+c', 'ab' * n + 'c'), (r'(?:(a)|(b)){64,}+c', 'ab' * n),
        (r'(?:a|ab){%d}+b' % n, 'a' * (n - 1) + 'ab'), (r'(?:a|ab){%d}+b' % n, 'a' * n + 'b'),
        (r'(?:ab|a){1,}+b', 'ab' * n + 'b'), (r'(?:(ab)|(a))*+(?(1)x|y)', 'ab' * n + 'y'),
        (r'(?>x((?:a|ab)*))b', 'x' + 'ab' * n + 'b'), (r'(?=((?:a|b)*))(.)', 'ab' * n),
        (r'(?:(a)|b|c()){%d,}+a' % (n - 1), 'ab' * n + 'c' + 'a'),
    ]:
        m = re.match(p, s)
        print(n, p, len(s), m and (m.span(), [g and g[-4:] for g in m.groups()], m.lastindex))
wide = '(?:a|b' + '()' * 300 + ')*+'
for s in ('b' + 'a' * 300, 'a' * 300 + 'b', 'ab' * 150):
    m = re.fullmatch(wide, s)
    print(len(s), m and (m.span(), m.groups()[:2], m.groups()[-1], m.lastindex))
