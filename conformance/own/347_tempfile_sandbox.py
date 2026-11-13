# tempfile exists so that modules importing it load; every call that would
# touch the filesystem raises the sandbox's PermissionError, as open() does
# (docs/user-guide.md "Known differences from CPython"). CPython creates real files here, so only
# the pure parts and the mm-side error are compared: the case prints what
# both engines agree on.
import tempfile
print(tempfile.gettempprefix(), tempfile.gettempprefixb(), tempfile.TMP_MAX, tempfile.template)
print(sorted(n for n in tempfile.__all__))
for name in ('mkdtemp', 'mkstemp', 'gettempdir', 'TemporaryDirectory', 'NamedTemporaryFile',
             'TemporaryFile', 'SpooledTemporaryFile', 'mktemp', 'gettempdirb'):
    print(name, callable(getattr(tempfile, name)))
try:
    import os
    d = tempfile.mkdtemp()
    os.rmdir(d)
    print('ok')
except PermissionError as e:
    print('ok' if 'no ambient filesystem' in str(e) else e)
