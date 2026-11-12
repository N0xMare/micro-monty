# re: syntax and semantics corners (str and bytes patterns).
import re

print(re.search(r'(?<=ab)c', 'xabc').span(), re.search(r'(?<!a)b', 'ab cb').start())
print(re.match(r'(a+)+b', 'aaab').groups(), re.fullmatch(r'a{2,3}?', 'aaa').group())
print(re.findall(r'a*+b', 'aaab ab b'), re.findall(r'(?>a+)b', 'aaab'), re.search(r'(?>a+)ab', 'aaab'))
print(re.sub(r'(\w+) (\w+)', r'\2 \1', 'hello world'), re.sub(r'(?P<x>\d)', r'<\g<x>>', 'a1b2'))
print(re.subn(r'\s+', '-', ' a  b \t c '), re.sub('x*', '-', 'abxd'))
print(re.split(r'(,)', 'a,b,,c'), re.split(r'\W+', 'Words, words, words.', maxsplit=1), re.split('x*', 'axbc'))
m = re.match(r'(?P<first>\w+) (?P<last>\w+)(?P<opt>!)?', 'Jane Doe')
print(m.groupdict(), m.groupdict('-'), m['first'], m[2], m.group(1, 2), m.span('last'), m.lastgroup, m.lastindex)
print(m.expand(r'\g<last>, \1'), m.re.pattern, m.string, m.pos, m.endpos, repr(m))
p = re.compile(r'(\d+)-(\d+)', re.I | re.M)
print(p.flags, p.groups, p.groupindex, p.pattern, repr(p))
print(re.findall(r'^\w', 'ab\ncd', re.M), re.findall(r'.', 'a\nb', re.S), re.findall(r'(?i)straße', 'STRASSE straße'))
print(re.findall(r'\w+', 'héllo wörld 123'), re.findall(r'\w+', 'héllo', re.A), re.findall(r'\d', '1٣'), re.findall(r'\d', '1٣', re.ASCII))
print(re.findall(r'\bfoo\b', 'foo foobar (foo)'), re.findall(r'\Bo\B', 'foo boo'))
print(re.search(r'(a)(?:b)(c)?', 'ab').groups(), re.search(r'(a)|(b)', 'b').groups('x'))
print(re.match(r'(\w)\1', 'aa').group(), re.match(r'(?P<c>\w)(?P=c)', 'bb').group(), re.match(r'(a)?(?(1)b|c)', 'c').group())
print(re.escape('a.b*c?d'), re.escape(b'x+y'), re.findall(rb'\d+', b'a12b3'))
print(re.match(r'''(?x) a  # comment
   b''', 'ab').group())
for pat in ['(', 'a**', '[a-', '(?P<1>a)', r'\k', '(?<=a+)b', 'a{2,1}', '(?P<n>a)(?P<n>b)']:
    try:
        re.compile(pat)
    except re.error as e:
        print(repr(pat), '->', e.msg, e.pos)
print([m.span() for m in re.finditer(r'a|', 'baa')])
print(re.sub(r'a', lambda m: m.group().upper() * 2, 'banana', count=2))
print(re.compile('a').match('ba', 1).span(), re.compile('^a').search('ba', 1))
print(bool(re.match(r'(?s:.)', '\n')), re.findall(r'(?-i:a)b', 'aB AB', re.I))
print(re.purge(), re.fullmatch(r'\d+', '12a'), type(re.compile('a')).__name__, type(re.match('a', 'a')).__name__)
