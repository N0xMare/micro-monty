// classes: 2,000 small objects in a list; 60 steps of method calls that read
// and write instance attributes, plus 200k constructor calls building new ones.
class Particle {
  constructor(i) {
    this.x = (i * 37) % 1000;
    this.y = (i * 91) % 1000;
    this.vx = (i % 7) + 1;
    this.vy = (i % 5) + 1;
    this.hits = 0;
  }

  step() {
    this.x += this.vx;
    this.y += this.vy;
    if (this.x >= 1000) {
      this.x -= 1000;
      this.hits += 1;
    }
    if (this.y >= 1000) {
      this.y -= 1000;
      this.hits += 1;
    }
  }

  energy() {
    return this.vx * this.vx + this.vy * this.vy;
  }
}

class Vec {
  constructor(x, y) {
    this.x = x;
    this.y = y;
  }

  add(other) {
    return new Vec(this.x + other.x, this.y + other.y);
  }

  dot(other) {
    return this.x * other.x + this.y * other.y;
  }
}

const particles = [];
for (let i = 0; i < 2000; i++) {
  particles.push(new Particle(i));
}
for (let s = 0; s < 60; s++) {
  for (const p of particles) {
    p.step();
  }
}

let hits = 0;
let energy = 0;
let position = 0;
for (const p of particles) {
  hits += p.hits;
  energy += p.energy();
  position += p.x * 3 + p.y;
}

let acc = new Vec(0, 0);
const step = new Vec(1, 2);
let dots = 0;
for (let i = 0; i < 200000; i++) {
  acc = acc.add(step);
  dots = (dots + acc.dot(step)) % 1000000007;
}
console.log(hits, energy, position, acc.x, acc.y, dots);
