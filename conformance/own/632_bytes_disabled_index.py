class Indexed:
    def __index__(self):
        return 3


class Disabled(Indexed):
    __index__ = None


for convert in (bytes, bytearray):
    assert len(convert(Indexed())) == 3
    try:
        convert(Disabled())
    except TypeError as exc:
        assert str(exc) == f"cannot convert 'Disabled' object to {convert.__name__}"
    else:
        raise AssertionError(convert)
