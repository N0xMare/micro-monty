# platform (fixed sandbox values; only what equals CPython is printed) and sys.path (docs/spec.md §12).
import platform
import sys

print(platform.python_version() == '%d.%d.%d' % sys.version_info[:3])
t = platform.python_version_tuple()
print(type(t).__name__, len(t), t == tuple(str(n) for n in sys.version_info[:3]), t[:2])
print(platform.python_version().startswith('3.14.'))
for f in [platform.system, platform.node, platform.release, platform.version, platform.machine,
          platform.processor, platform.platform, platform.python_implementation, platform.python_compiler,
          platform.python_branch, platform.python_revision]:
    print(f.__name__, type(f()).__name__)
print(platform.system() == 'Linux' if sys.platform == 'linux' else True)
u = platform.uname()
print(type(u).__name__, isinstance(u, tuple), len(u), u.system == platform.system(), u[0] == u.system)
print(u.node == platform.node(), u.release == platform.release(), u.version == platform.version())
print(u.machine == platform.machine(), type(u.processor).__name__, tuple(u) == u[:])
print(platform.uname() == u, repr(u).startswith('uname_result(system='))
b = platform.python_build()
print(type(b).__name__, len(b), [type(x).__name__ for x in b])
a = platform.architecture()
print(type(a).__name__, len(a), a[0] in ('32bit', '64bit'))
print(type(platform.libc_ver()).__name__, len(platform.libc_ver()))
print(platform.platform(terse=True) == platform.platform(terse=True), platform.system().startswith('Linux') or True)

# sys.path: a list code may read and edit (imports do not search it in micro-monty).
print(type(sys.path).__name__, all(isinstance(p, str) for p in sys.path))
saved = sys.path[:]
sys.path.insert(0, '/nonexistent-dir')
print(sys.path[0])
sys.path[:] = saved
print(sys.path == saved, '/nonexistent-dir' in sys.path)
import json
print(json.dumps([1]))
