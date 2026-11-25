from io import BytesIO, StringIO


class Index:
    def __init__(self, value):
        self.value = value

    def __index__(self):
        return self.value


for stream in (BytesIO(b'abc'), StringIO('abc')):
    stream.seek(1)
    try:
        stream.truncate(-1)
    except ValueError as exc:
        print(str(exc), stream.tell(), stream.getvalue())
    try:
        stream.truncate(Index('2'))
    except TypeError as exc:
        print(str(exc), stream.tell(), stream.getvalue())
    print(stream.truncate(Index(2)), stream.tell(), stream.getvalue())
    print(stream.truncate(), stream.tell(), stream.getvalue())
