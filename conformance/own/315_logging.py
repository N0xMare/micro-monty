# logging (slim): levels, the hierarchy, handlers, formatters in the three
# styles, filters, basicConfig and exception() (docs/spec.md §4, §12).
import logging, sys, io
log = logging.getLogger('app')
log.warning('no handlers: %s', 'last resort')
log.info('hidden')
print(log, logging.getLogger(), logging.getLogger('app') is log, log.parent is logging.root)
logging.basicConfig(level=logging.DEBUG, stream=sys.stdout)
log.debug('debug %d', 1)
log.info('info %s', 'x')
logging.warning('module-level')
child = logging.getLogger('app.sub.leaf')
mid = logging.getLogger('app.sub')
print(child.parent.name, mid.parent.name, log.getChild('sub') is mid)
buf = io.StringIO()
h = logging.StreamHandler(buf)
h.setFormatter(logging.Formatter('[%(levelname)-8s] %(name)s: %(message)s (%(levelno)d)'))
h.setLevel(logging.INFO)
mid.addHandler(h)
child.debug('child debug')
child.error('child error %r', {'a': 1})
child.critical('dict args %(x)s', {'x': 5})
mid.propagate = False
child.info('not to root')
print(repr(buf.getvalue()))
try:
    1 / 0
except ZeroDivisionError:
    log.exception('failed %s', 'here')
    log.error('with exc', exc_info=True)
e = ValueError('instance')
log.error('exc instance', exc_info=e)
f = logging.Formatter('{levelname}|{name}|{message}', style='{')
h2 = logging.StreamHandler(sys.stdout)
h2.setFormatter(f)
other = logging.getLogger('other')
other.propagate = False
other.addHandler(h2)
other.warning('brace style')
h2.setFormatter(logging.Formatter('$levelname ${name} $message', style='$'))
other.warning('dollar style')
h2.setFormatter(logging.Formatter('%(message)s %(custom)s', defaults={'custom': 'dflt'}))
other.warning('defaults')
other.warning('extra', extra={'custom': 'given'})
try:
    other.warning('x', extra={'message': 1})
except KeyError as e:
    print('KeyError', e)
for bad in [lambda: logging.Formatter('no fields'), lambda: logging.Formatter('x', style='?'),
            lambda: logging.Formatter('nofields', style='{'), lambda: log.setLevel('LOUD'),
            lambda: log.setLevel(1.5), lambda: logging.basicConfig(foo=1, force=True),
            lambda: logging.basicConfig(stream=sys.stdout, filename='x', force=True),
            lambda: log.log('INFO', 'x')]:
    try:
        bad()
    except (ValueError, TypeError) as e:
        print(type(e).__name__, e)
print(logging.getLevelName(10), logging.getLevelName('INFO'), logging.getLevelName(15))
logging.addLevelName(15, 'VERBOSE')
log.log(15, 'verbose %s', 'msg')
print(logging.getLevelName(15), log.getEffectiveLevel(), log.isEnabledFor(5), log.hasHandlers())
log.setLevel('ERROR')
log.warning('suppressed')
print(log, logging.root)
class F(logging.Filter):
    def filter(self, record):
        return 'secret' not in record.getMessage()
other.addFilter(F())
other.warning('secret thing')
other.warning('public thing')
other.addFilter(lambda r: r.levelno > logging.WARNING)
other.warning('dropped by callable')
other.error('kept by callable')
print(logging.Filter('a.b').filter(logging.makeLogRecord({'name': 'a.bc'})),
      logging.Filter('a.b').filter(logging.makeLogRecord({'name': 'a.b.c'})))
r = logging.makeLogRecord({'name': 'n', 'msg': 'm %s', 'args': (1,), 'levelno': 20})
print(r.getMessage(), r.name, r.lineno, r.funcName, r.pathname, r.filename, r.module)
rec = logging.LogRecord('n', 20, '/a/b/c.py', 7, 'hi %s', ('x',), None, 'fn')
print(rec, rec.filename, rec.module, rec.levelname, rec.getMessage())
logging.disable(logging.CRITICAL)
other.critical('disabled')
logging.disable(logging.NOTSET)
other.critical('enabled again')
print(logging.NullHandler(), logging.lastResort)
nh = logging.getLogger('quiet')
nh.addHandler(logging.NullHandler())
nh.warning('to null')
logging.basicConfig(format='%(name)s->%(message)s', force=True, stream=sys.stdout, level='INFO')
logging.info('forced %s', 'config')
logging.getLogger('x.y').info('nested')
import re
cap = io.StringIO()
th = logging.StreamHandler(cap)
th.setFormatter(logging.Formatter('%(asctime)s %(msecs)d %(relativeCreated)d %(message)s'))
tl = logging.getLogger('timed')
tl.addHandler(th)
tl.warning('stamped')
print(bool(re.fullmatch(r'\d{4}-\d\d-\d\d \d\d:\d\d:\d\d,\d{3} \d+ \d+ stamped\n', cap.getvalue())))
th.setFormatter(logging.Formatter('%(asctime)s|%(message)s', datefmt='%Y'))
tl.warning('year')
print(bool(re.fullmatch(r'\d{4}\|year\n', cap.getvalue().split('stamped\n')[1])))
rec = logging.LogRecord('n', 20, 'p', 1, 'm', None, None)
print(type(rec.created).__name__, type(rec.msecs).__name__, rec.created > 1e9)
import time
logging.Formatter.converter = time.gmtime
print(logging.Formatter('%(asctime)s', datefmt='%Z').format(rec))
