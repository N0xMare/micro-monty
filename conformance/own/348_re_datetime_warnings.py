# The DeprecationWarning and FutureWarning CPython's re and datetime emit:
# positional count/maxsplit/flags in re.sub/subn/split, possible set
# operations and nested sets in patterns, utcnow() and utcfromtimestamp()
# (docs/spec.md §12). Shown warnings go to stderr, which is not compared.
import re, warnings, datetime
with warnings.catch_warnings(record=True) as ws:
    warnings.simplefilter('always')
    print(re.sub('a', 'b', 'aaaaa', 1), re.subn('a', 'b', 'aaa', 2), re.split(':', 'a:b:c', 1))
    print(re.sub('a', 'b', 'aaaaa', count=1), re.split(':', 'a:b:c', maxsplit=1))
    print(re.findall(r'[0-9--1]', '-./0123456789'))
    re.compile(r'[a&&b]'); re.compile(r'[a~~b]'); re.compile(r'[a||b]'); re.compile(r'[[a]'); re.compile(r'[%--]')
    re.compile(r'[--1]')
    d = datetime.datetime.utcfromtimestamp(0)
    n = datetime.datetime.utcnow()
    print(d, type(n).__name__)
    datetime.datetime.now(); datetime.datetime.fromtimestamp(0, datetime.UTC)
for w in ws:
    print(w.category.__name__, w.message)
with warnings.catch_warnings():
    warnings.simplefilter('error')
    for f, a in ((re.compile, (r'[a--b]',)), (re.sub, ('x', 'y', 'xx', 1)),
                 (datetime.datetime.utcfromtimestamp, (86400,)), (re.compile, (r'[%--b]',))):
        try:
            f(*a)
            print('no warning', a)
        except (DeprecationWarning, FutureWarning) as e:
            print(type(e).__name__, str(e)[:40])
with warnings.catch_warnings(record=True) as ws:
    warnings.simplefilter('always')
    re.compile(r'[%--b]')
    re.compile(r'[%--b]', re.I)
    print([str(w.message) for w in ws])
with warnings.catch_warnings(record=True) as ws:
    warnings.simplefilter('always')
    print(datetime.datetime.strptime('03-14.5', '%m-%d.%f'))
    print(datetime.datetime.strptime('20-03-14', '%y-%m-%d'), datetime.datetime.strptime('02-29,2024', '%m-%d,%Y'))
    print(datetime.datetime.strptime('03-14.5', '%m-%d.%f'))
    print([(w.category.__name__, str(w.message).splitlines()[0]) for w in ws])
