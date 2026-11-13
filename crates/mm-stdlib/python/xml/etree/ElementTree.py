# xml.etree.ElementTree: after CPython 3.14's Lib/xml/etree/ElementTree.py
# (THIRD_PARTY.md). Element, SubElement, Comment, PI, QName, ElementTree,
# TreeBuilder, the serializer (xml, html, text), indent, dump, XML,
# fromstring, fromstringlist, XMLID, parse and register_namespace are
# CPython's. XMLParser is micro-monty's: a small non-validating parser in
# place of expat, with expat's messages, codes and positions for the
# errors it reports. It expands the five predefined entities, character
# references and the internal entities a DOCTYPE declares, and leaves
# out iterparse, XMLPullParser and canonicalize. Its notice:
#
# Copyright (c) 1999-2008 by Fredrik Lundh.  All rights reserved.
#
# By obtaining, using, and/or copying this software and/or its
# associated documentation, you agree that you have read, understood,
# and will comply with the following terms and conditions:
#
# Permission to use, copy, modify, and distribute this software and
# its associated documentation for any purpose and without fee is
# hereby granted, provided that the above copyright notice appears in
# all copies, and that both that copyright notice and this permission
# notice appear in supporting documentation, and that the name of
# Secret Labs AB or the author not be used in advertising or publicity
# pertaining to distribution of the software without specific, written
# prior permission.
#
# SECRET LABS AB AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD
# TO THIS SOFTWARE, INCLUDING ALL IMPLIED WARRANTIES OF MERCHANT-
# ABILITY AND FITNESS.  IN NO EVENT SHALL SECRET LABS AB OR THE AUTHOR
# BE LIABLE FOR ANY SPECIAL, INDIRECT OR CONSEQUENTIAL DAMAGES OR ANY
# DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
# WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS
# ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE
# OF THIS SOFTWARE.
#
# Licensed to PSF under a Contributor Agreement.
# See https://www.python.org/psf/license for licensing details.

__all__ = [
    "Comment",
    "dump",
    "Element", "ElementTree",
    "fromstring", "fromstringlist",
    "indent", "iselement",
    "parse", "ParseError",
    "PI", "ProcessingInstruction",
    "QName",
    "SubElement",
    "tostring", "tostringlist",
    "TreeBuilder",
    "VERSION",
    "XML", "XMLID",
    "XMLParser",
    "register_namespace",
    ]

VERSION = "1.3.0"

import sys
import re

from xml.etree import ElementPath


class ParseError(SyntaxError):
    pass


def iselement(element):
    return hasattr(element, 'tag')


class Element:
    tag = None
    attrib = None
    text = None
    tail = None

    def __init__(self, tag, attrib={}, **extra):
        if not isinstance(attrib, dict):
            raise TypeError("attrib must be dict, not %s" % (
                attrib.__class__.__name__,))
        self.tag = tag
        self.attrib = {**attrib, **extra}
        self._children = []

    def __repr__(self):
        return "<%s %r at %#x>" % (self.__class__.__name__, self.tag, id(self))

    def makeelement(self, tag, attrib):
        return self.__class__(tag, attrib)

    def __copy__(self):
        elem = self.makeelement(self.tag, self.attrib)
        elem.text = self.text
        elem.tail = self.tail
        elem[:] = self
        return elem

    def __len__(self):
        return len(self._children)

    def __bool__(self):
        import warnings
        warnings.warn(
            "Testing an element's truth value will always return True in "
            "future versions.  "
            "Use specific 'len(elem)' or 'elem is not None' test instead.",
            DeprecationWarning, stacklevel=2
            )
        return len(self._children) != 0

    def __iter__(self):
        return iter(self._children)

    def __getitem__(self, index):
        return self._children[index]

    def __setitem__(self, index, element):
        if isinstance(index, slice):
            for elt in element:
                self._assert_is_element(elt)
        else:
            self._assert_is_element(element, None)
        self._children[index] = element

    def __delitem__(self, index):
        del self._children[index]

    def append(self, subelement):
        self._assert_is_element(subelement, "append() argument 1")
        self._children.append(subelement)

    def extend(self, elements):
        for element in elements:
            self._assert_is_element(element)
            self._children.append(element)

    def insert(self, index, subelement):
        self._assert_is_element(subelement, "insert() argument 2")
        self._children.insert(index, subelement)

    # CPython's C Element's messages.
    def _assert_is_element(self, e, what=""):
        if not isinstance(e, Element):
            if what:
                raise TypeError('%s must be xml.etree.ElementTree.Element, not %s'
                                % (what, type(e).__name__))
            raise TypeError('expected an Element, not "%s"' % type(e).__name__)

    def remove(self, subelement):
        self._assert_is_element(subelement, "remove() argument")
        try:
            self._children.remove(subelement)
        except ValueError:
            raise ValueError("Element.remove(x): element not found") from None

    def find(self, path, namespaces=None):
        return ElementPath.find(self, path, namespaces)

    def findtext(self, path, default=None, namespaces=None):
        return ElementPath.findtext(self, path, default, namespaces)

    def findall(self, path, namespaces=None):
        return ElementPath.findall(self, path, namespaces)

    def iterfind(self, path, namespaces=None):
        return ElementPath.iterfind(self, path, namespaces)

    def clear(self):
        self.attrib.clear()
        self._children = []
        self.text = self.tail = None

    def get(self, key, default=None):
        return self.attrib.get(key, default)

    def set(self, key, value):
        self.attrib[key] = value

    # Lists, as CPython's C Element gives them.
    def keys(self):
        return list(self.attrib)

    def items(self):
        return list(self.attrib.items())

    def iter(self, tag=None):
        if tag == "*":
            tag = None
        if tag is None or self.tag == tag:
            yield self
        for e in self._children:
            yield from e.iter(tag)

    def itertext(self):
        tag = self.tag
        if not isinstance(tag, str) and tag is not None:
            return
        t = self.text
        if t:
            yield t
        for e in self:
            yield from e.itertext()
            t = e.tail
            if t:
                yield t


def SubElement(parent, tag, attrib={}, **extra):
    Element._assert_is_element(None, parent, "SubElement() argument 1")
    attrib = {**attrib, **extra}
    element = parent.makeelement(tag, attrib)
    parent.append(element)
    return element


def Comment(text=None):
    element = Element(Comment)
    element.text = text
    return element


def ProcessingInstruction(target, text=None):
    element = Element(ProcessingInstruction)
    element.text = target
    if text:
        element.text = element.text + " " + text
    return element

PI = ProcessingInstruction


class QName:
    def __init__(self, text_or_uri, tag=None):
        if tag:
            text_or_uri = "{%s}%s" % (text_or_uri, tag)
        self.text = text_or_uri
    def __str__(self):
        return self.text
    def __repr__(self):
        return '<%s %r>' % (self.__class__.__name__, self.text)
    def __hash__(self):
        return hash(self.text)
    def __le__(self, other):
        if isinstance(other, QName):
            return self.text <= other.text
        return self.text <= other
    def __lt__(self, other):
        if isinstance(other, QName):
            return self.text < other.text
        return self.text < other
    def __ge__(self, other):
        if isinstance(other, QName):
            return self.text >= other.text
        return self.text >= other
    def __gt__(self, other):
        if isinstance(other, QName):
            return self.text > other.text
        return self.text > other
    def __eq__(self, other):
        if isinstance(other, QName):
            return self.text == other.text
        return self.text == other


def _broken_path(path):
    import warnings
    path = "." + path
    warnings.warn(
        "This search is broken in 1.3 and earlier, and will be "
        "fixed in a future version.  If you rely on the current "
        "behaviour, change it to %r" % path,
        FutureWarning, stacklevel=3
        )
    return path


class ElementTree:
    def __init__(self, element=None, file=None):
        if element is not None and not iselement(element):
            raise TypeError('expected an Element, not %s' %
                            type(element).__name__)
        self._root = element
        if file:
            self.parse(file)

    def getroot(self):
        return self._root

    def _setroot(self, element):
        if not iselement(element):
            raise TypeError('expected an Element, not %s'
                            % type(element).__name__)
        self._root = element

    def parse(self, source, parser=None):
        close_source = False
        if not hasattr(source, "read"):
            source = open(source, "rb")
            close_source = True
        try:
            if parser is None:
                parser = XMLParser()
            while data := source.read(65536):
                parser.feed(data)
            self._root = parser.close()
            return self._root
        finally:
            if close_source:
                source.close()

    def iter(self, tag=None):
        return self._root.iter(tag)

    def find(self, path, namespaces=None):
        if path[:1] == "/":
            path = _broken_path(path)
        return self._root.find(path, namespaces)

    def findtext(self, path, default=None, namespaces=None):
        if path[:1] == "/":
            path = _broken_path(path)
        return self._root.findtext(path, default, namespaces)

    def findall(self, path, namespaces=None):
        if path[:1] == "/":
            path = _broken_path(path)
        return self._root.findall(path, namespaces)

    def iterfind(self, path, namespaces=None):
        if path[:1] == "/":
            path = _broken_path(path)
        return self._root.iterfind(path, namespaces)

    def write(self, file_or_filename,
              encoding=None,
              xml_declaration=None,
              default_namespace=None,
              method=None, *,
              short_empty_elements=True):
        if self._root is None:
            raise TypeError('ElementTree not initialized')
        if not method:
            method = "xml"
        elif method not in _serialize:
            raise ValueError("unknown method %r" % method)
        if not encoding:
            encoding = "us-ascii"
        close = None
        if hasattr(file_or_filename, "write"):
            file = file_or_filename
        else:
            file = close = open(file_or_filename, "w" if encoding.lower() == "unicode" else "wb")
        try:
            if encoding.lower() == "unicode":
                write = file.write
            else:
                def write(text):
                    file.write(text.encode(encoding, "xmlcharrefreplace"))
            if method == "xml" and (xml_declaration or
                    (xml_declaration is None and
                     encoding.lower() != "unicode" and
                     encoding.lower() not in ("utf-8", "us-ascii"))):
                write("<?xml version='1.0' encoding='%s'?>\n" % (
                    "utf-8" if encoding.lower() == "unicode" else encoding,))
            if method == "text":
                _serialize_text(write, self._root)
            else:
                qnames, namespaces = _namespaces(self._root, default_namespace)
                serialize = _serialize[method]
                serialize(write, self._root, qnames, namespaces,
                          short_empty_elements=short_empty_elements)
        finally:
            if close is not None:
                close.close()


def _namespaces(elem, default_namespace=None):
    qnames = {None: None}
    namespaces = {}
    if default_namespace:
        namespaces[default_namespace] = ""

    def add_qname(qname):
        try:
            if qname[:1] == "{":
                uri, tag = qname[1:].rsplit("}", 1)
                prefix = namespaces.get(uri)
                if prefix is None:
                    prefix = _namespace_map.get(uri)
                    if prefix is None:
                        prefix = "ns%d" % len(namespaces)
                    if prefix != "xml":
                        namespaces[uri] = prefix
                if prefix:
                    qnames[qname] = "%s:%s" % (prefix, tag)
                else:
                    qnames[qname] = tag
            else:
                if default_namespace:
                    raise ValueError(
                        "cannot use non-qualified names with "
                        "default_namespace option"
                        )
                qnames[qname] = qname
        except TypeError:
            _raise_serialization_error(qname)

    for elem in elem.iter():
        tag = elem.tag
        if isinstance(tag, QName):
            if tag.text not in qnames:
                add_qname(tag.text)
        elif isinstance(tag, str):
            if tag not in qnames:
                add_qname(tag)
        elif tag is not None and tag is not Comment and tag is not PI:
            _raise_serialization_error(tag)
        for key, value in elem.items():
            if isinstance(key, QName):
                key = key.text
            if key not in qnames:
                add_qname(key)
            if isinstance(value, QName) and value.text not in qnames:
                add_qname(value.text)
        text = elem.text
        if isinstance(text, QName) and text.text not in qnames:
            add_qname(text.text)
    return qnames, namespaces


def _start_tag(write, tag, elem, qnames, namespaces, escape):
    write("<" + tag)
    items = list(elem.items())
    if items or namespaces:
        if namespaces:
            for v, k in sorted(namespaces.items(),
                               key=lambda x: x[1]):
                if k:
                    k = ":" + k
                write(" xmlns%s=\"%s\"" % (
                    k,
                    _escape_attrib(v)
                    ))
        for k, v in items:
            if isinstance(k, QName):
                k = k.text
            if isinstance(v, QName):
                v = qnames[v.text]
            else:
                v = escape(v)
            write(" %s=\"%s\"" % (qnames[k], v))


def _serialize_xml(write, elem, qnames, namespaces,
                   short_empty_elements, **kwargs):
    tag = elem.tag
    text = elem.text
    if tag is Comment:
        write("<!--%s-->" % text)
    elif tag is ProcessingInstruction:
        write("<?%s?>" % text)
    else:
        tag = qnames[tag]
        if tag is None:
            if text:
                write(_escape_cdata(text))
            for e in elem:
                _serialize_xml(write, e, qnames, None,
                               short_empty_elements=short_empty_elements)
        else:
            _start_tag(write, tag, elem, qnames, namespaces, _escape_attrib)
            if text or len(elem) or not short_empty_elements:
                write(">")
                if text:
                    write(_escape_cdata(text))
                for e in elem:
                    _serialize_xml(write, e, qnames, None,
                                   short_empty_elements=short_empty_elements)
                write("</" + tag + ">")
            else:
                write(" />")
    if elem.tail:
        write(_escape_cdata(elem.tail))

_CDATA_CONTENT_ELEMENTS = {"script", "style", "xmp", "iframe", "noembed",
                           "noframes", "plaintext"}

HTML_EMPTY = {"area", "base", "basefont", "br", "col", "embed", "frame", "hr",
              "img", "input", "isindex", "link", "meta", "param", "source",
              "track", "wbr", "plaintext"}

def _serialize_html(write, elem, qnames, namespaces, **kwargs):
    tag = elem.tag
    text = elem.text
    if tag is Comment:
        write("<!--%s-->" % _escape_cdata(text))
    elif tag is ProcessingInstruction:
        write("<?%s?>" % _escape_cdata(text))
    else:
        tag = qnames[tag]
        if tag is None:
            if text:
                write(_escape_cdata(text))
            for e in elem:
                _serialize_html(write, e, qnames, None)
        else:
            _start_tag(write, tag, elem, qnames, namespaces, _escape_attrib_html)
            write(">")
            ltag = tag.lower()
            if text:
                if ltag in _CDATA_CONTENT_ELEMENTS:
                    write(text)
                else:
                    write(_escape_cdata(text))
            for e in elem:
                _serialize_html(write, e, qnames, None)
            if ltag not in HTML_EMPTY:
                write("</" + tag + ">")
    if elem.tail:
        write(_escape_cdata(elem.tail))

def _serialize_text(write, elem):
    for part in elem.itertext():
        write(part)
    if elem.tail:
        write(elem.tail)

_serialize = {
    "xml": _serialize_xml,
    "html": _serialize_html,
    "text": _serialize_text,
}


def register_namespace(prefix, uri):
    if re.match(r"ns\d+$", prefix):
        raise ValueError("Prefix format reserved for internal use")
    for k, v in list(_namespace_map.items()):
        if k == uri or v == prefix:
            del _namespace_map[k]
    _namespace_map[uri] = prefix

_namespace_map = {
    "http://www.w3.org/XML/1998/namespace": "xml",
    "http://www.w3.org/1999/xhtml": "html",
    "http://www.w3.org/1999/02/22-rdf-syntax-ns#": "rdf",
    "http://schemas.xmlsoap.org/wsdl/": "wsdl",
    "http://www.w3.org/2001/XMLSchema": "xs",
    "http://www.w3.org/2001/XMLSchema-instance": "xsi",
    "http://purl.org/dc/elements/1.1/": "dc",
}

def _raise_serialization_error(text):
    raise TypeError(
        "cannot serialize %r (type %s)" % (text, type(text).__name__)
        )

def _escape_cdata(text):
    try:
        if "&" in text:
            text = text.replace("&", "&amp;")
        if "<" in text:
            text = text.replace("<", "&lt;")
        if ">" in text:
            text = text.replace(">", "&gt;")
        return text
    except (TypeError, AttributeError):
        _raise_serialization_error(text)

def _escape_attrib(text):
    try:
        if "&" in text:
            text = text.replace("&", "&amp;")
        if "<" in text:
            text = text.replace("<", "&lt;")
        if ">" in text:
            text = text.replace(">", "&gt;")
        if "\"" in text:
            text = text.replace("\"", "&quot;")
        if "\r" in text:
            text = text.replace("\r", "&#13;")
        if "\n" in text:
            text = text.replace("\n", "&#10;")
        if "\t" in text:
            text = text.replace("\t", "&#09;")
        return text
    except (TypeError, AttributeError):
        _raise_serialization_error(text)

def _escape_attrib_html(text):
    try:
        if "&" in text:
            text = text.replace("&", "&amp;")
        if ">" in text:
            text = text.replace(">", "&gt;")
        if "\"" in text:
            text = text.replace("\"", "&quot;")
        return text
    except (TypeError, AttributeError):
        _raise_serialization_error(text)


class _Collect:
    def __init__(self, lst):
        self.lst = lst
    def write(self, b):
        self.lst.append(b)


def tostring(element, encoding=None, method=None, *,
             xml_declaration=None, default_namespace=None,
             short_empty_elements=True):
    lst = tostringlist(element, encoding, method,
                       xml_declaration=xml_declaration,
                       default_namespace=default_namespace,
                       short_empty_elements=short_empty_elements)
    return ("" if encoding == "unicode" else b"").join(lst)

def tostringlist(element, encoding=None, method=None, *,
                 xml_declaration=None, default_namespace=None,
                 short_empty_elements=True):
    lst = []
    ElementTree(element).write(_Collect(lst), encoding,
                               xml_declaration=xml_declaration,
                               default_namespace=default_namespace,
                               method=method,
                               short_empty_elements=short_empty_elements)
    return lst


def dump(elem):
    if not isinstance(elem, ElementTree):
        elem = ElementTree(elem)
    elem.write(sys.stdout, encoding="unicode")
    tail = elem.getroot().tail
    if not tail or tail[-1] != "\n":
        sys.stdout.write("\n")


def indent(tree, space="  ", level=0):
    if isinstance(tree, ElementTree):
        tree = tree.getroot()
    if level < 0:
        raise ValueError(f"Initial indentation level must be >= 0, got {level}")
    if not len(tree):
        return

    indentations = ["\n" + level * space]

    def _indent_children(elem, level):
        child_level = level + 1
        try:
            child_indentation = indentations[child_level]
        except IndexError:
            child_indentation = indentations[level] + space
            indentations.append(child_indentation)

        if not elem.text or not elem.text.strip():
            elem.text = child_indentation

        for child in elem:
            if len(child):
                _indent_children(child, child_level)
            if not child.tail or not child.tail.strip():
                child.tail = child_indentation

        if not child.tail.strip():
            child.tail = indentations[level]

    _indent_children(tree, 0)


def parse(source, parser=None):
    tree = ElementTree()
    tree.parse(source, parser)
    return tree


def XML(text, parser=None):
    if not parser:
        parser = XMLParser(target=TreeBuilder())
    parser.feed(text)
    return parser.close()


def XMLID(text, parser=None):
    if not parser:
        parser = XMLParser(target=TreeBuilder())
    parser.feed(text)
    tree = parser.close()
    ids = {}
    for elem in tree.iter():
        id = elem.get("id")
        if id:
            ids[id] = elem
    return tree, ids

fromstring = XML

def fromstringlist(sequence, parser=None):
    if not parser:
        parser = XMLParser(target=TreeBuilder())
    for text in sequence:
        parser.feed(text)
    return parser.close()


class TreeBuilder:
    def __init__(self, element_factory=None, *,
                 comment_factory=None, pi_factory=None,
                 insert_comments=False, insert_pis=False):
        self._data = []
        self._elem = []
        self._last = None
        self._root = None
        self._tail = None
        if comment_factory is None:
            comment_factory = Comment
        self._comment_factory = comment_factory
        self.insert_comments = insert_comments
        if pi_factory is None:
            pi_factory = ProcessingInstruction
        self._pi_factory = pi_factory
        self.insert_pis = insert_pis
        if element_factory is None:
            element_factory = Element
        self._factory = element_factory

    def close(self):
        assert len(self._elem) == 0, "missing end tags"
        assert self._root is not None, "missing toplevel element"
        return self._root

    def _flush(self):
        if self._data:
            if self._last is not None:
                text = "".join(self._data)
                if self._tail:
                    assert self._last.tail is None, "internal error (tail)"
                    self._last.tail = text
                else:
                    assert self._last.text is None, "internal error (text)"
                    self._last.text = text
            self._data = []

    def data(self, data):
        self._data.append(data)

    def start(self, tag, attrs):
        self._flush()
        self._last = elem = self._factory(tag, attrs)
        if self._elem:
            self._elem[-1].append(elem)
        elif self._root is None:
            self._root = elem
        self._elem.append(elem)
        self._tail = 0
        return elem

    def end(self, tag):
        self._flush()
        self._last = self._elem.pop()
        assert self._last.tag == tag,\
               "end tag mismatch (expected %s, got %s)" % (
                   self._last.tag, tag)
        self._tail = 1
        return self._last

    def comment(self, text):
        return self._handle_single(
            self._comment_factory, self.insert_comments, text)

    def pi(self, target, text=None):
        return self._handle_single(
            self._pi_factory, self.insert_pis, target, text)

    def _handle_single(self, factory, insert, *args):
        elem = factory(*args)
        if insert:
            self._flush()
            self._last = elem
            if self._elem:
                self._elem[-1].append(elem)
            self._tail = 1
        return elem


# The parser. Names follow XML's Name production loosely: a letter or
# underscore, then word characters, '.', '-' and U+00B7; with namespaces a
# name is an NCName or prefix:NCName, as expat checks it.
_NAME = re.compile(r"[^\W\d][\w.\-\u00b7:]*")
_SPACE = re.compile(r"[ \t\n]*")
_TEXT = re.compile(r"[^<&]+")
_RUN = re.compile(r"[^<&]*")
_HEX = re.compile(r"[0-9a-fA-F]+")
_DEC = re.compile(r"[0-9]+")
_ENTITY = re.compile(r"<!ENTITY\s+([^\s%]\S*)\s+(\"[^\"]*\"|'[^']*')\s*>")
_EXTERNAL = re.compile(r"\s(SYSTEM|PUBLIC)\s")
_BAD_TEXT = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\ufffe\uffff]|\]\]>")
_XML_NS = "http://www.w3.org/XML/1998/namespace"
_PREDEFINED = {"lt": "<", "gt": ">", "amp": "&", "quot": '"', "apos": "'"}


class _Error(Exception):
    def __init__(self, message, code, at):
        self.message = message
        self.code = code
        self.at = at


def _fail(message, code, at):
    raise _Error(message, code, at)

def _invalid(at):
    raise _Error("not well-formed (invalid token)", 4, at)

def _unclosed(at):
    raise _Error("unclosed token", 5, at)


class XMLParser:
    def __init__(self, *, target=None, encoding=None):
        if target is None:
            target = TreeBuilder()
        self.target = self._target = target
        self._encoding = encoding
        self._parts = []
        self.entity = {}
        self.version = "micro-monty"

    def feed(self, data):
        self._parts.append(data)

    def flush(self):
        pass

    def close(self):
        parts = self._parts
        self._parts = []
        if parts and not isinstance(parts[0], str):
            data = self._decode(b"".join(parts))
        else:
            data = "".join(parts)
        parse = _Parse(self, data)
        try:
            parse.document()
        except _Error as e:
            raise _error(e.message, e.code, parse.s, e.at) from None
        try:
            close_handler = self.target.close
        except AttributeError:
            pass
        else:
            return close_handler()

    def _decode(self, data):
        encoding = self._encoding
        if data[:3] == b"\xef\xbb\xbf":
            data = data[3:]
        elif data[:2] in (b"\xff\xfe", b"\xfe\xff"):
            encoding = "utf-16"
        if encoding is None:
            m = re.match(rb"<\?xml[^>]*?encoding\s*=\s*[\"']([A-Za-z][\w.\-]*)", data)
            encoding = m.group(1).decode() if m else "utf-8"
        try:
            return data.decode(encoding)
        except LookupError:
            raise _error("unknown encoding", 18, "", 0) from None
        except UnicodeDecodeError as e:
            text = data[:e.start].decode(encoding, "replace")
            raise _error("not well-formed (invalid token)", 4, text, len(text)) from None


def _error(message, code, s, at):
    line = s.count("\n", 0, at) + 1
    column = at - s.rfind("\n", 0, at) - 1
    err = ParseError("%s: line %d, column %d" % (message, line, column))
    err.code = code
    err.position = line, column
    return err


class _Parse:
    def __init__(self, parser, data):
        if data[:1] == "\ufeff":
            data = data[1:]
        if "\r" in data:
            data = data.replace("\r\n", "\n").replace("\r", "\n")
        self.s = data
        self.target = target = parser.target
        self.entity = parser.entity
        self.declared = None
        self.data = getattr(target, "data", None)
        self.start = getattr(target, "start", None)
        self.end = getattr(target, "end", None)
        self.comment_ = getattr(target, "comment", None)
        self.pi_ = getattr(target, "pi", None)
        self.start_ns = getattr(target, "start_ns", None)
        self.end_ns = getattr(target, "end_ns", None)

    def document(self):
        s = self.s
        n = len(s)
        i = 0
        if s.startswith("<?xml") and s[5:6] in (" ", "\t", "\n", "?"):
            j = s.find("?>")
            if j < 0:
                _unclosed(0)
            i = j + 2
        root = False
        while True:
            i = _SPACE.match(s, i).end()
            if i >= n:
                if root:
                    return
                _fail("no element found", 3, n)
            if s.startswith("<!--", i):
                i = self.comment(i)
            elif s.startswith("<?", i):
                i = self.pi(i)
            elif root:
                if s[i] == "<" and i + 1 == n:
                    _unclosed(i)
                _fail("junk after document element", 9, i)
            elif s.startswith("<!DOCTYPE", i):
                i = self.doctype(i)
            elif s[i] == "<":
                i = self.element(i)
                root = True
            else:
                m = _NAME.match(s, i)
                if m and m.end() == n:
                    _fail("syntax error", 2, i)
                _invalid(m.end() if m else i)

    def comment(self, i):
        s = self.s
        j = s.find("--", i + 4)
        if j < 0:
            _unclosed(i)
        if s[j + 2:j + 3] != ">":
            if j + 2 >= len(s):
                _unclosed(i)
            _invalid(j + 2)
        if self.comment_ is not None:
            self.comment_(s[i + 4:j])
        return j + 3

    def pi(self, i):
        s = self.s
        m = _NAME.match(s, i + 2)
        if not m:
            _invalid(i + 2)
        name = m.group()
        if name.lower() == "xml":
            _fail("XML or text declaration not at start of entity", 17, i)
        j = s.find("?>", m.end())
        if j < 0:
            _unclosed(i)
        text = s[m.end():j]
        if text and text[0] not in " \t\n":
            _invalid(m.end())
        if self.pi_ is not None:
            self.pi_(name, text.lstrip(" \t\n"))
        return j + 2

    def doctype(self, i):
        # The name, an external id and the internal subset, whose general
        # entities with a literal value are kept; the rest is skipped.
        s = self.s
        n = len(s)
        self.declared = {}
        j = i + 9
        while j < n and s[j] not in "[>":
            if s[j] in "\"'":
                k = s.find(s[j], j + 1)
                if k < 0:
                    _unclosed(i)
                j = k
            j += 1
        if j >= n:
            _unclosed(i)
        self.external = bool(_EXTERNAL.search(s[i + 9:j]))
        if s[j] == ">":
            return j + 1
        j += 1
        while True:
            j = _SPACE.match(s, j).end()
            if j >= n:
                _unclosed(i)
            if s[j] == "]":
                k = _SPACE.match(s, j + 1).end()
                if s[k:k + 1] != ">":
                    _unclosed(i) if k >= n else _invalid(k)
                return k + 1
            if s.startswith("<!--", j):
                j = s.find("-->", j + 4)
                if j < 0:
                    _unclosed(i)
                j += 3
            elif s.startswith("<?", j):
                j = s.find("?>", j + 2)
                if j < 0:
                    _unclosed(i)
                j += 2
            elif s.startswith("<!", j):
                m = _ENTITY.match(s, j)
                if m:
                    self.declared.setdefault(m.group(1), m.group(2)[1:-1])
                    j = m.end()
                else:
                    k = j + 2
                    while k < n and s[k] != ">":
                        if s[k] in "\"'":
                            q = s.find(s[k], k + 1)
                            if q < 0:
                                _unclosed(i)
                            k = q
                        k += 1
                    if k >= n:
                        _unclosed(i)
                    j = k + 1
            elif s[j] == "%":
                m = _NAME.match(s, j + 1)
                if not m or s[m.end():m.end() + 1] != ";":
                    _invalid(m.end() if m else j + 1)
                j = m.end() + 1
            else:
                _invalid(j)

    def qname(self, i, end):
        # The end of the name at i: an NCName, or two joined by one ':'.
        s = self.s
        m = _NAME.match(s, i)
        if not m:
            if s[i:i + 1] == ":" or i < len(s):
                _invalid(i)
            _unclosed(end)
        j = m.end()
        colon = s.find(":", i, j)
        if colon >= 0:
            if colon == i:
                _invalid(i)
            if colon == j - 1:
                _invalid(j)
            second = s.find(":", colon + 1, j)
            if second >= 0:
                _invalid(second)
        return j

    def element(self, start):
        # Elements without recursion: a stack of open tags.
        s = self.s
        n = len(s)
        stack = []
        scopes = [{"xml": _XML_NS}]
        i = start
        while True:
            # A start tag at s[i] == '<'.
            j = self.qname(i + 1, i)
            raw = s[i + 1:j]
            attrs = []
            names = set()
            while True:
                k = _SPACE.match(s, j).end()
                if k >= n:
                    _unclosed(i)
                c = s[k]
                if c == ">" or c == "/":
                    break
                if k == j:
                    _invalid(k)
                j = self.qname(k, i)
                name = s[k:j]
                if name in names:
                    _fail("duplicate attribute", 8, k)
                names.add(name)
                j = _SPACE.match(s, j).end()
                if j >= n:
                    _unclosed(i)
                if s[j] != "=":
                    _invalid(j)
                j = _SPACE.match(s, j + 1).end()
                if j >= n:
                    _unclosed(i)
                q = s[j]
                if q != '"' and q != "'":
                    _invalid(j)
                e = s.find(q, j + 1)
                if e < 0:
                    _unclosed(i)
                attrs.append((name, self.value(j + 1, e, i)))
                j = e + 1
            empty = c == "/"
            if empty:
                if s[k + 1:k + 2] != ">":
                    if k + 1 >= n:
                        _unclosed(i)
                    _invalid(k + 1)
                k += 1
            # Namespaces.
            scope = scopes[-1]
            declared = None
            for name, value in attrs:
                if name == "xmlns" or name[:6] == "xmlns:":
                    if declared is None:
                        declared = []
                        scope = dict(scope)
                    prefix = name[6:]
                    if prefix and not value:
                        _fail("must not undeclare prefix", 28, i)
                    scope[prefix] = value
                    declared.append(prefix)
            attrib = {}
            for name, value in attrs:
                if name == "xmlns" or name[:6] == "xmlns:":
                    continue
                if ":" in name:
                    prefix, local = name.split(":")
                    uri = scope.get(prefix)
                    if not uri:
                        _fail("unbound prefix", 27, i)
                    name = "{%s}%s" % (uri, local)
                    if name in attrib:
                        _fail("duplicate attribute", 8, i)
                attrib[name] = value
            if ":" in raw:
                prefix, local = raw.split(":")
                uri = scope.get(prefix)
                if not uri:
                    _fail("unbound prefix", 27, i)
                tag = "{%s}%s" % (uri, local)
            else:
                uri = scope.get("")
                tag = "{%s}%s" % (uri, raw) if uri else raw
            if declared and self.start_ns is not None:
                for prefix in declared:
                    self.start_ns(prefix, scope[prefix])
            if self.start is not None:
                self.start(tag, attrib)
            i = k + 1
            if empty:
                if self.end is not None:
                    self.end(tag)
                self.ended(declared)
            else:
                stack.append((raw, tag, declared))
                scopes.append(scope)
            # Content up to the next start tag, or the end of the root.
            while stack:
                if i >= n:
                    _fail("no element found", 3, n)
                c = s[i]
                if c == "<":
                    if s.startswith("</", i):
                        raw, tag, declared = stack[-1]
                        j = self.qname(i + 2, i)
                        if s[i + 2:j] != raw:
                            _fail("mismatched tag", 7, i + 2)
                        k = _SPACE.match(s, j).end()
                        if k >= n:
                            _unclosed(i)
                        if s[k] != ">":
                            _invalid(k)
                        i = k + 1
                        stack.pop()
                        scopes.pop()
                        if self.end is not None:
                            self.end(tag)
                        self.ended(declared)
                    elif s.startswith("<!--", i):
                        i = self.comment(i)
                    elif s.startswith("<![CDATA[", i):
                        j = s.find("]]>", i + 9)
                        if j < 0:
                            _fail("unclosed CDATA section", 20, n)
                        self.text(s[i + 9:j], i + 9)
                        i = j + 3
                    elif s.startswith("<?", i):
                        i = self.pi(i)
                    elif s.startswith("<!", i):
                        _invalid(i + 2) if i + 2 < n else _unclosed(i)
                    else:
                        break
                elif c == "&":
                    i = self.reference(i)
                else:
                    j = _TEXT.match(s, i).end()
                    self.text(s[i:j], i)
                    i = j
            if not stack:
                return i

    def ended(self, declared):
        if declared and self.end_ns is not None:
            for prefix in reversed(declared):
                self.end_ns(prefix)

    def text(self, text, at):
        m = _BAD_TEXT.search(text)
        if m:
            _invalid(at + m.start() + (2 if m.group() == "]]>" else 0))
        if self.data is not None:
            self.data(text)

    def reference(self, i):
        # The reference at s[i] == '&' in content; the index after it.
        s = self.s
        if s[i + 1:i + 2] == "#":
            j, char = self.charref(i)
            if self.data is not None:
                self.data(char)
            return j
        m = _NAME.match(s, i + 1)
        if not m:
            _invalid(i + 1) if i + 1 < len(s) else _unclosed(i)
        j = m.end()
        if s[j:j + 1] != ";":
            _invalid(j) if j < len(s) else _unclosed(i)
        name = m.group()
        value = _PREDEFINED.get(name)
        if value is None and self.declared is not None:
            value = self.declared.get(name)
            if value is not None:
                value = self.value_text(value)
            elif self.external:
                value = self.entity.get(name)
        if value is None:
            if self.declared is not None and self.external:
                _fail("undefined entity &%s;" % name, 11, i)
            _fail("undefined entity", 11, i)
        if self.data is not None:
            self.data(value)
        return j + 1

    def charref(self, i):
        s = self.s
        if s[i + 2:i + 3] == "x":
            m = _HEX.match(s, i + 3)
            base = 16
            j = i + 3
        else:
            m = _DEC.match(s, i + 2)
            base = 10
            j = i + 2
        if m:
            j = m.end()
        if not m or s[j:j + 1] != ";":
            _invalid(j) if j < len(s) else _unclosed(i)
        code = int(m.group(), base)
        if not (code in (9, 10, 13) or 0x20 <= code <= 0xd7ff
                or 0xe000 <= code <= 0xfffd or 0x10000 <= code <= 0x10ffff):
            _fail("reference to invalid character number", 14, i)
        return j + 1, chr(code)

    def value(self, i, e, tag):
        # The attribute value s[i:e], references expanded and whitespace
        # normalized.
        s = self.s
        out = []
        while i < e:
            c = s[i]
            if c == "<":
                _invalid(i)
            if c == "&":
                if s[i + 1:i + 2] == "#":
                    i, char = self.charref(i)
                    out.append(char)
                    continue
                m = _NAME.match(s, i + 1)
                if not m or m.end() >= e or s[m.end()] != ";":
                    _invalid(m.end() if m else i + 1)
                name = m.group()
                value = _PREDEFINED.get(name)
                if value is None and self.declared is not None:
                    value = self.declared.get(name)
                    if value is not None:
                        value = self.value_text(value)
                if value is None:
                    _fail("undefined entity", 11, tag)
                out.append(value)
                i = m.end() + 1
                continue
            j = _RUN.match(s, i, e).end()
            m = _BAD_TEXT.search(s, i, j)
            if m and m.group() != "]]>":
                _invalid(m.start())
            out.append(s[i:j].replace("\t", " ").replace("\n", " "))
            i = j
        return "".join(out)

    def value_text(self, value):
        # A declared entity's replacement text: its character references
        # and the predefined entities expanded.
        return re.sub(r"&(#x[0-9a-fA-F]+|#[0-9]+|lt|gt|amp|quot|apos);",
                      lambda m: _PREDEFINED.get(m.group(1)) or
                      chr(int(m.group(1)[2:], 16) if m.group(1)[1] == "x" else int(m.group(1)[1:])),
                      value)
