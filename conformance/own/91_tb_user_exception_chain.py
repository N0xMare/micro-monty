# Uncaught user exceptions with chaining, custom __str__, raised in methods.
class ConfigError(Exception):
    def __init__(self, key):
        super().__init__(key)
        self.key = key

    def __str__(self):
        return f'missing config key {self.key!r}'


class LoadError(RuntimeError):
    pass


class Loader:
    def __init__(self, data):
        self.data = data

    def get(self, key):
        try:
            return self.data[key]
        except KeyError:
            raise ConfigError(key)

    def load(self):
        try:
            return self.get('port')
        except ConfigError as e:
            raise LoadError('cannot load') from e


Loader({'host': 'x'}).load()
