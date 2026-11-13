# xml.etree.ElementTree: the parser in place of expat, paths, text and
# serialization, as model-written code uses them (RSS, Atom, sitemaps).
import xml.etree.ElementTree as ET

feed = '''<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>News</title>
    <item id="1"><title>A &amp;amp; <b>B</b> c</title>
      <pubDate>Mon, 02 Mar 2026 10:00:00 +0100</pubDate></item>
    <item><title><![CDATA[<i>raw</i> & text]]></title><!-- note --></item>
  </channel>
</rss>
'''
root = ET.fromstring(feed)
print(root.tag, root.attrib, repr(root.text), len(root), len(root[0]))
for item in root.findall('.//item'):
    title = item.find('title')
    print(item.get('id'), item.get('x', 'none'), repr(item.findtext('title')),
          repr(''.join(title.itertext())), repr(item.findtext('pubDate', 'no date')))
print([e.tag for e in root.iter()], [e.tag for e in root.iter('title')])
print(root.find('channel/item[@id="1"]/title/b').tail, root.findall('channel/item[2]')[0].findtext('title'))
print(root.find('missing'), root.findtext('missing'), root.findtext('channel/title'))
print(sorted(root.keys()), root.items(), root[0][1].keys())

sitemap = ('<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" '
           'xmlns:img="http://www.google.com/schemas/sitemap-image/1.1">'
           '<url><loc>https://shop.example.com/p?id=3</loc><lastmod>2026-03-07</lastmod>'
           '<img:image img:caption="c">x</img:image></url><url><loc>u2</loc></url></urlset>')
root = ET.fromstring(sitemap)
ns = '{http://www.sitemaps.org/schemas/sitemap/0.9}'
print(root.tag)
print([u.findtext('{*}loc') for u in root.findall('{*}url')])
print([u.findtext(ns + 'lastmod', '-') for u in root.findall(ns + 'url')])
print([e.tag for e in root.iter() if e.tag.endswith('image')], root[0][2].attrib)
print(root.findall('sm:url/sm:loc', {'sm': ns[1:-1]})[1].text)

atom = '<feed xmlns="http://www.w3.org/2005/Atom"><entry><title type="html">a &lt;b&gt;</title></entry></feed>'
for e in ET.fromstring(atom).iter('{http://www.w3.org/2005/Atom}entry'):
    print(e.findtext('{http://www.w3.org/2005/Atom}title'))

# Text and attribute values: references, line ends, whitespace.
r = ET.fromstring('<a b=" x\ty\nz " c=\'&quot;&#65;&#x42;\'>\r\nl1\rl2&lt;&gt;&apos;&#233;</a>')
print(r.attrib, repr(r.text))
r = ET.fromstring('<!DOCTYPE a [<!ENTITY who "W &amp; X"><!ELEMENT a ANY>]><a>hi &who;</a>')
print(repr(r.text))
print(ET.fromstring('<a xml:lang="en"><b xmlns="u"><c xmlns=""/></b></a>').iter().__next__().attrib)
print([e.tag for e in ET.fromstring('<a xml:lang="en"><b xmlns="u"><c xmlns=""/></b></a>').iter()])

# Errors, with expat's messages, codes and positions.
for bad in ['', '   ', '<a>', '<a></b>', '<a/><b/>', 'abc', ' x<a/>', '<a>&nbsp;</a>',
            '<a b="1" b="2"/>', '<a>&</a>', '<a b=1/>', '<x:a/>', '<a', '<a>\n  <b>&foo;</b></a>',
            '<a>&#0;</a>', '<a/>junk', '<a b="<"/>', '<a>]]></a>', '<!-- x -- y --><a/>',
            ' <?xml version="1.0"?><a/>', '<a xmlns:p=""/>', '<a><![CDATA[x</a>',
            '<!DOCTYPE a SYSTEM "a.dtd"><a>&foo;</a>', '<a:b:c xmlns:a="u"/>']:
    try:
        ET.fromstring(bad)
        print(repr(bad), 'parsed')
    except ET.ParseError as e:
        print(repr(bad), e, e.code, e.position, isinstance(e, SyntaxError))

# Building and writing.
e = ET.Element('x', {'a': '1'}, b='2')
s = ET.SubElement(e, 'y', q='"<&>"\n')
s.text = 'hi <&>'
s.tail = 't'
ET.SubElement(e, 'z')
print(ET.tostring(e, encoding='unicode'))
print(ET.tostring(e))
print(ET.tostring(e, method='text', encoding='unicode'))
print(ET.tostring(ET.fromstring('<a b="&quot;é"/>'), encoding='utf-8'))
ET.indent(e)
print(ET.tostring(e, encoding='unicode'))
q = ET.Element('{urn:x}root')
ET.SubElement(q, '{urn:x}leaf').text = 'v'
print(ET.tostring(q, encoding='unicode'))
tree = ET.ElementTree(ET.fromstring('<a><b>1</b><b>2</b></a>'))
print(tree.getroot().tag, tree.findtext('b'), [b.text for b in tree.iter('b')], len(tree.findall('b')))
root, ids = ET.XMLID('<a><b id="k">1</b></a>')
print(sorted(ids), ids['k'].text)
del e[0]
e.remove(e[0])
print(len(e), ET.iselement(e), ET.iselement('x'))
for bad in [lambda: e.append('x'), lambda: e.insert(0, 'x'), lambda: e.extend(['x']),
            lambda: e.remove('x'), lambda: e.remove(ET.Element('q')), lambda: ET.SubElement('x', 'y'),
            lambda: ET.ElementTree('x')]:
    try:
        bad()
    except (TypeError, ValueError) as err:
        print(type(err).__name__, err)
