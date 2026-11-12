class A: pass
setattr(A, 'x' * (M // 4), 1)
try:
    A.y
except AttributeError:
    import traceback
    t = traceback.format_exc()
