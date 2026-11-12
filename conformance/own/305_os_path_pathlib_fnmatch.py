# The pure parts of os.path, pathlib and fnmatch.
import fnmatch
import os
import os.path
from pathlib import PurePath, PurePosixPath

for p in ['/a/b/c.txt', 'a/b/', 'c', '', '/', '//x/./y/../z', 'a.tar.gz', '.bashrc']:
    print(repr(p), os.path.split(p), os.path.splitext(p), os.path.basename(p), os.path.dirname(p), os.path.normpath(p), os.path.isabs(p))
print(os.path.join('a', 'b', '/c', 'd'), os.path.join('a', ''), os.path.commonpath(['/a/b/c', '/a/b/d']), os.path.commonprefix(['abc', 'abd']))
print(os.path.relpath('/a/b/c', '/a/d'), os.path.splitroot('//a/b'), os.sep, os.pathsep, os.curdir, os.fspath(PurePath('x/y')))
p = PurePosixPath('/usr/local/lib/python3.tar.gz')
print(p, repr(p), p.parts, p.parent, list(p.parents)[:2], p.name, p.stem, p.suffix, p.suffixes, p.anchor, p.root)
print(p.with_name('x.py'), p.with_suffix('.zip'), p.with_stem('y'), p.relative_to('/usr'), p.is_relative_to('/opt'), p / 'more' / '..')
print(PurePath('a') / 'b', 'pre' / PurePath('b'), PurePath('a/b').match('*/b'), PurePath('a/b.py').full_match('**/*.py'), PurePath('/a') == PurePosixPath('/a'))
print(sorted([PurePath('b'), PurePath('a/c'), PurePath('a')]), PurePath('a', 'b', 'c').as_posix(), hash(PurePath('a')) == hash(PurePath('a')))
print(fnmatch.fnmatch('foo.py', '*.py'), fnmatch.fnmatchcase('Foo.PY', '*.py'), fnmatch.filter(['a.c', 'b.h', 'c.c'], '*.c'), fnmatch.translate('[!a-c]*.?'))
for bad in [lambda: PurePath('a').relative_to('b'), lambda: PurePath('a').with_suffix('x'), lambda: os.path.join('a', 1), lambda: PurePath(1)]:
    try:
        bad()
    except (ValueError, TypeError) as e:
        print(type(e).__name__, e)
