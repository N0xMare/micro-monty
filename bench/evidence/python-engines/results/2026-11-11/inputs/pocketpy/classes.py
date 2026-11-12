# classes: 2,000 small objects in a list; 60 steps of method calls that read
# and write instance attributes, plus 200k `__init__` calls building new ones.
class Particle:
    def __init__(self, i):
        self.x = i * 37 % 1000
        self.y = i * 91 % 1000
        self.vx = i % 7 + 1
        self.vy = i % 5 + 1
        self.hits = 0

    def step(self):
        self.x += self.vx
        self.y += self.vy
        if self.x >= 1000:
            self.x -= 1000
            self.hits += 1
        if self.y >= 1000:
            self.y -= 1000
            self.hits += 1

    def energy(self):
        return self.vx * self.vx + self.vy * self.vy


class Vec:
    def __init__(self, x, y):
        self.x = x
        self.y = y

    def add(self, other):
        return Vec(self.x + other.x, self.y + other.y)

    def dot(self, other):
        return self.x * other.x + self.y * other.y


particles = [Particle(i) for i in range(2000)]
for _ in range(60):
    for p in particles:
        p.step()

hits = 0
energy = 0
position = 0
for p in particles:
    hits += p.hits
    energy += p.energy()
    position += p.x * 3 + p.y

acc = Vec(0, 0)
step = Vec(1, 2)
dots = 0
for i in range(200000):
    acc = acc.add(step)
    dots = (dots + acc.dot(step)) % 1000000007
print(hits, energy, position, acc.x, acc.y, dots)
