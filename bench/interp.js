// interp: a tiny language end to end: generate 300 assignment statements,
// tokenize them (character loop, Token objects), parse them by recursive
// descent into arrays, and evaluate the tree against a Map of variables
// through an operator table. 25 rounds of all three.
class Token {
  constructor(kind, text) {
    this.kind = kind;
    this.text = text;
  }
}

const isSpace = (c) => c === ' ' || c === '\n' || c === '\t' || c === '\r';
const isDigit = (c) => c >= '0' && c <= '9';
const isAlpha = (c) => (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z');
const isAlnum = (c) => isAlpha(c) || isDigit(c);

function tokenize(source) {
  const tokens = [];
  let i = 0;
  const n = source.length;
  while (i < n) {
    const c = source[i];
    if (isSpace(c)) {
      i += 1;
    } else if (isDigit(c)) {
      const start = i;
      while (i < n && isDigit(source[i])) i += 1;
      tokens.push(new Token('num', source.slice(start, i)));
    } else if (isAlpha(c)) {
      const start = i;
      while (i < n && isAlnum(source[i])) i += 1;
      tokens.push(new Token('name', source.slice(start, i)));
    } else if ('+-*%=();'.includes(c)) {
      tokens.push(new Token('op', c));
      i += 1;
    } else {
      throw new SyntaxError('bad character ' + c);
    }
  }
  tokens.push(new Token('end', ''));
  return tokens;
}

class Parser {
  constructor(tokens) {
    this.tokens = tokens;
    this.pos = 0;
  }

  peek() {
    return this.tokens[this.pos];
  }

  take(text) {
    const token = this.tokens[this.pos];
    if (token.text !== text) {
      throw new SyntaxError('expected ' + text + ', got ' + token.text);
    }
    this.pos += 1;
  }

  program() {
    const statements = [];
    while (this.peek().kind !== 'end') {
      const name = this.peek().text;
      this.pos += 1;
      this.take('=');
      statements.push([name, this.expr()]);
      this.take(';');
    }
    return statements;
  }

  expr() {
    let node = this.term();
    while (this.peek().text === '+' || this.peek().text === '-') {
      const op = this.peek().text;
      this.pos += 1;
      node = ['bin', op, node, this.term()];
    }
    return node;
  }

  term() {
    let node = this.atom();
    while (this.peek().text === '*' || this.peek().text === '%') {
      const op = this.peek().text;
      this.pos += 1;
      node = ['bin', op, node, this.atom()];
    }
    return node;
  }

  atom() {
    const token = this.peek();
    this.pos += 1;
    if (token.kind === 'num') return ['num', parseInt(token.text, 10)];
    if (token.kind === 'name') return ['var', token.text];
    if (token.text === '(') {
      const node = this.expr();
      this.take(')');
      return node;
    }
    throw new SyntaxError('unexpected ' + token.text);
  }
}

// `%` floors like Python's, so negative operands give the same results.
const OPS = new Map([
  ['+', (a, b) => a + b],
  ['-', (a, b) => a - b],
  ['*', (a, b) => a * b],
  ['%', (a, b) => ((a % b) + b) % b],
]);

function evaluate(node, env) {
  const tag = node[0];
  if (tag === 'num') return node[1];
  if (tag === 'var') return env.get(node[1]);
  return OPS.get(node[1])(evaluate(node[2], env), evaluate(node[3], env));
}

function run(source) {
  const statements = new Parser(tokenize(source)).program();
  const env = new Map();
  for (let i = 0; i < 20; i++) env.set(`v${i}`, i + 1);
  for (let round = 0; round < 4; round++) {
    for (const [name, node] of statements) {
      env.set(name, evaluate(node, env));
    }
  }
  let sum = 0;
  for (const value of env.values()) sum += value;
  return [statements.length, sum];
}

const lines = [];
for (let k = 0; k < 300; k++) {
  lines.push(`v${k % 20} = (v${(k * 7) % 20} + ${k}) * v${(k * 3) % 20} % 10007 - ${k % 13};`);
}
const source = lines.join('\n');

let total = 0;
let count = 0;
for (let r = 0; r < 25; r++) {
  const [n, s] = run(source.split('10007').join(String(10007 + r * 2)));
  count += n;
  total += s;
}
console.log(count, total);
