# html.parser (CPython's, with _markupbase): every handler, script/style CDATA,
# charrefs in attributes, chunked feeds with convert_charrefs=False, getpos.
from html.parser import HTMLParser
class P(HTMLParser):
    def __init__(self):
        super().__init__()
        self.events = []
    def handle_starttag(self, tag, attrs): self.events.append(('start', tag, attrs))
    def handle_endtag(self, tag): self.events.append(('end', tag))
    def handle_startendtag(self, tag, attrs): self.events.append(('startend', tag, attrs))
    def handle_data(self, data): self.events.append(('data', data))
    def handle_comment(self, data): self.events.append(('comment', data))
    def handle_decl(self, data): self.events.append(('decl', data))
    def handle_pi(self, data): self.events.append(('pi', data))
    def handle_entityref(self, name): self.events.append(('entity', name))
    def handle_charref(self, name): self.events.append(('charref', name))
    def unknown_decl(self, data): self.events.append(('unknown', data))
doc = '''<!DOCTYPE html><html><head><title>T &amp; t</title>
<script>if (a < b) { x = "</p>"; }</script><style>p{}</style></head>
<body class="main" data-x='1 &lt; 2' checked><!-- c --><p>Hello <b>world</b> &copy; &#169; &#xa9;<br/>
<a href="/x?a=1&b=2">link</a><img src=x.png alt=""><?php echo 1 ?><![CDATA[raw]]></p>
<ul><li>one<li>two</ul></body></html>'''
p = P(); p.feed(doc); p.close()
for e in p.events: print(e)
p = P(); p.convert_charrefs = False
for chunk in ['<p>a &am', 'p; b', ' &#6', '5;</p', '>']:
    p.feed(chunk)
p.close()
print(p.events)
print(p.getpos())
class L(HTMLParser):
    def __init__(self):
        super().__init__(); self.links = []
    def handle_starttag(self, tag, attrs):
        if tag == 'a':
            self.links.extend(v for k, v in attrs if k == 'href')
l = L(); l.feed('<div><a href="https://a.example/1">1</a> <A HREF=/two>2</A></div>'); print(l.links)
