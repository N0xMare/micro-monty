def risky(kind):
    if kind == 0:
        return 1 / 0
    if kind == 1:
        return [][1]
    if kind == 2:
        return {}["k"]
    if kind == 3:
        return int("abc")
    if kind == 4:
        return undefined_name
    if kind == 5:
        return "a" + 1
    if kind == 6:
        return (1).nope
    raise RuntimeError("custom", 42)

for k in range(8):
    try:
        risky(k)
    except (ZeroDivisionError, IndexError) as e:
        print("arith/index:", type(e).__name__, e)
    except LookupError as e:
        print("lookup:", type(e).__name__, repr(e))
    except Exception as e:
        print("other:", type(e).__name__, e, e.args)
    else:
        print("no error")
    finally:
        print("finally", k)

# Exception arguments can cycle through native str(), including both OSError
# fields. Every route must raise a catchable error, not overflow the Rust stack.
import sys
saved_limit = sys.getrecursionlimit()
sys.setrecursionlimit(100)
for kind in range(4):
    e = ValueError('seed')
    if kind == 0:
        e.args = (e,)
    elif kind == 1:
        e.args = (RuntimeError(e),)
    elif kind == 2:
        e.args = (OSError(e, 'message'),)
    else:
        e.args = (OSError(2, e),)
    for render in [str, lambda x: format(x, ''), lambda x: f'{x}']:
        try:
            render(e)
        except RecursionError:
            pass
        else:
            raise AssertionError('recursive exception str did not fail')
    print('exception cycle bounded', kind)

# Ordinary nesting and subsequent rendering still work after each refusal.
sys.setrecursionlimit(saved_limit)
e = ValueError('leaf')
for i in range(300):
    e = ValueError(e)
assert str(e) == 'leaf'
assert str(OSError(ValueError(2), ValueError('missing'))) == '[Errno 2] missing'
print('exception rendering recovered')
