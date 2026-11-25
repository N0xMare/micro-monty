import traceback


def leaf():
    raise ValueError


try:
    leaf()
except ValueError as e:
    for limit in (False, True, -1, 2**50):
        print([frame.name for frame in traceback.extract_tb(e.__traceback__, limit)])
