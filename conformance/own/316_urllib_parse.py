# urllib.parse: CPython 3.14's parsing, joining, quoting and query-string
# functions (docs/spec.md §4, §12).
from urllib.parse import *
import urllib.parse as up
import urllib
print(urllib.parse is up)
for u in ['https://user:pw@Example.COM:8080/a/b;p?q=1&r=2#frag', 'http://[::1]:80/x', '//host/p',
          'mailto:a@b.c', 'file:///etc/x', '/rel/path?x', 'http://h/%7Euser', 'HTTP://h', 'a:b']:
    r = urlparse(u)
    print(r, r.hostname, r.port, r.username, r.password, r.geturl())
    s = urlsplit(u)
    print(s, urlunsplit(s), urlunparse(r))
print(urlparse(b'http://a/b?c'), urlsplit(b'http://a/b').decode(), urlsplit('http://a/b').encode())
for base, rel in [('http://a/b/c/d;p?q', 'g'), ('http://a/b/c/d;p?q', '../g'), ('http://a/b/c/d;p?q', '//g'),
                  ('http://a/b/c/d;p?q', '?y'), ('http://a/b/c/d;p?q', '#s'), ('http://a/b/c/d;p?q', '../../../g'),
                  ('http://a/b/', 'http://other/x'), ('', 'x'), ('http://a/b', '')]:
    print(urljoin(base, rel))
print(urldefrag('http://a/b#c'), urldefrag('x'))
print(quote('a b/c?d=é&'), quote('a b/c', safe=''), quote_plus('a b&c=d/'), quote(b'\x00\xff'))
print(unquote('a%20b%2Fc%C3%A9%zz'), unquote_plus('a+b%20c'), unquote_to_bytes('%41%zz'), unquote(b'%41'))
print(unquote('%E9', encoding='latin-1'), unquote('%ff'), quote_from_bytes(b'abc def'))
print(urlencode({'a': 1, 'b': 'x y', 'c': ['1', '2']}), urlencode({'c': ['1', '2']}, doseq=True))
print(urlencode([('a', 'b&c'), ('d', b'e f')]), urlencode({'q': 'a/b'}, safe='/'), urlencode({'q': 'a b'}, quote_via=quote))
print(parse_qs('a=1&a=2&b=&c=x+y%20z'), parse_qs('a=1&b=', keep_blank_values=True))
print(parse_qsl('a=1;b=2', separator=';'), parse_qsl(b'a=1&b=%41'), parse_qsl(''), parse_qsl(None))
for bad in [lambda: parse_qsl('a', strict_parsing=True), lambda: urlparse('http://[::1'),
            lambda: urlsplit('http://[1.2.3.4]/'), lambda: urlsplit('http://[zz]/'),
            lambda: urlparse('http://h:99999').port, lambda: urlparse('http://h:x').port,
            lambda: urljoin('http://a', b'b'), lambda: parse_qs('a=1', max_num_fields=0),
            lambda: quote(5), lambda: quote_from_bytes('x'), lambda: parse_qsl(5),
            lambda: urlencode('abc'), lambda: urlsplit('http://[v1.x]/'), lambda: urlsplit('http://[vz]/')]:
    try:
        print('ok', bad())
    except (ValueError, TypeError) as e:
        print(type(e).__name__, e)
print(urlsplit('http://[fe80::1%25eth0]:8/').hostname, urlsplit('http://[::ffff:1.2.3.4]/').hostname)
print(urlsplit(' http://a\tb/c\n'), up.uses_netloc[:3], up.scheme_chars[-4:])
print(SplitResult('a', 'b', 'c', 'd', 'e')._replace(path='z'), ParseResult._fields)
try:
    import urllib.nope
except ModuleNotFoundError as e:
    print(e)
