# html: escape and unescape, numeric and named references (the named table is
# mod-html-entities, which mm enables; docs/spec.md §4, §12).
import html
print(html.escape('<a href="x">Tom & \'Jerry\'</a>'), html.escape('<"\'>', quote=False))
for s in ['&amp;&lt;&gt;&quot;&apos;&#39;&#x27;', '&#65;&#x42;&#X43;&#0;&#13;&#128;&#x81;&#xD800;&#x110000;&#1;&#xFFFF;',
          '&#65&#x42 no semicolon', 'plain', '&', '&;', '&#;', '&#x;', '&amp', '&AMPx', '&ltfoo', '&nbsp;&copy;&eacute;&hellip;&notit;&NotEqualTilde;',
          '&nbsp&copy x', '&zzzz;', '&' + 'a' * 40 + ';', 'a & b && c', '&#9999999999;', '&Gt;', html.escape("<x y='1'>&</x>")]:
    print(repr(s), repr(html.unescape(s)))
print(html.unescape(html.escape('&<>"\'')) == '&<>"\'')
import html.entities as e
print(len(e.html5), e.name2codepoint['amp'], e.codepoint2name[0x3c], e.entitydefs['nbsp'] == '\xa0')
try:
    import html.nope
except ModuleNotFoundError as x:
    print(x)
