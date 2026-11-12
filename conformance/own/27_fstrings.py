name = "Ada"
n = 42
pi = 3.14159265
print(f"{name} is {n}")
print(f"{name!r} {name!s} {n!r}")
print(f"{pi:.2f} {pi:10.3f}| {n:5d}| {n:<5}| {n:^7}| {n:>4}| {n:05d} {n:x} {n:X} {n:o} {n:b}")
print(f"{n:,} {1234567:,} {1234567.891:,.2f} {-5:+d} {5:+d} {0.25:%} {0.25:.1%}")
print(f"{'s':*^9} {'left':<8}| {'right':>8}| {1e20:e} {12345.6789:g} {0.0001:g}")
print(f"{n + 1} {[1, 2][0]} {'nested ' + name} {{literal}}")
w = 10
print(f"{pi:{w}.{3}f}|")
print(f"{3.0} {1e16} {None} {True} {[1.5, 'x']}")
print(f"{name=}" if False else "skip-debug")
print(f"{n:>{w}}|", f"{'abc':.2}", f"{-0.0}")
