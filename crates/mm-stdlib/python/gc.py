# gc: micro-monty's collector is a tracing mark-sweep run by the engine
# (docs/spec.md §9.2); there are no generations or thresholds to tune.
# `collect()` runs a collection that starts after the call, a step per
# safepoint, and, having no count of what it freed, returns 0. The rest is
# CPython's API surface.

from _mm import gc_collect as _gc_collect, gc_wait as _gc_wait

_enabled = True
garbage = []
callbacks = []

DEBUG_STATS = 1
DEBUG_COLLECTABLE = 2
DEBUG_UNCOLLECTABLE = 4
DEBUG_SAVEALL = 32
DEBUG_LEAK = 38


def collect(generation=2):
    if not isinstance(generation, int):
        raise TypeError(f"'{type(generation).__name__}' object cannot be interpreted as an integer")
    if generation < 0 or generation > 2:
        raise ValueError('invalid generation')
    _gc_collect()
    # Over 32 MiB live the collection marks a step per safepoint, so a park
    # can come between steps; it has ended when this returns.
    while _gc_wait():
        pass
    return 0


def enable():
    global _enabled
    _enabled = True


def disable():
    global _enabled
    _enabled = False


def isenabled():
    return _enabled


def get_count():
    return (0, 0, 0)


def get_threshold():
    return (2000, 10, 0)


def set_threshold(threshold0, threshold1=None, threshold2=None, /):
    pass


def get_debug():
    return 0


def set_debug(flags, /):
    pass


def is_tracked(obj, /):
    return not isinstance(obj, (int, float, complex, str, bytes, bool, type(None)))


def is_finalized(obj, /):
    return False


def freeze():
    pass


def unfreeze():
    pass


def get_freeze_count():
    return 0
