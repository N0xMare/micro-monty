# An uncaught error deep in delegated generators called from methods.
class Pipeline:
    def __init__(self, data):
        self.data = data

    def stage1(self):
        for item in self.data:
            yield self.parse(item)

    def stage2(self):
        yield from self.stage1()

    def parse(self, item):
        return int(item)

    def run(self):
        return [v * 2 for v in self.stage2()]


print(Pipeline(['1', '2']).run())
Pipeline(['3', 'x']).run()
