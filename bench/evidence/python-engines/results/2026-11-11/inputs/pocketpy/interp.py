# interp: a tiny language end to end: generate 300 assignment statements,
# tokenize them (character loop, str methods, Token objects), parse them by
# recursive descent into tuples, and evaluate the tree against a dict of
# variables through an operator table. 25 rounds of all three.
class Token:
    def __init__(self, kind, text):
        self.kind = kind
        self.text = text


def tokenize(source):
    tokens = []
    i = 0
    n = len(source)
    while i < n:
        c = source[i]
        if c.isspace():
            i += 1
        elif c.isdigit():
            start = i
            while i < n and source[i].isdigit():
                i += 1
            tokens.append(Token('num', source[start:i]))
        elif c.isalpha():
            start = i
            while i < n and source[i].isalnum():
                i += 1
            tokens.append(Token('name', source[start:i]))
        elif c in '+-*%=();':
            tokens.append(Token('op', c))
            i += 1
        else:
            raise SyntaxError('bad character ' + c)
    tokens.append(Token('end', ''))
    return tokens


class Parser:
    def __init__(self, tokens):
        self.tokens = tokens
        self.pos = 0

    def peek(self):
        return self.tokens[self.pos]

    def take(self, text):
        token = self.tokens[self.pos]
        if token.text != text:
            raise SyntaxError('expected ' + text + ', got ' + token.text)
        self.pos += 1

    def program(self):
        statements = []
        while self.peek().kind != 'end':
            name = self.peek().text
            self.pos += 1
            self.take('=')
            statements.append((name, self.expr()))
            self.take(';')
        return statements

    def expr(self):
        node = self.term()
        while self.peek().text in ('+', '-'):
            op = self.peek().text
            self.pos += 1
            node = ('bin', op, node, self.term())
        return node

    def term(self):
        node = self.atom()
        while self.peek().text in ('*', '%'):
            op = self.peek().text
            self.pos += 1
            node = ('bin', op, node, self.atom())
        return node

    def atom(self):
        token = self.peek()
        self.pos += 1
        if token.kind == 'num':
            return ('num', int(token.text))
        if token.kind == 'name':
            return ('var', token.text)
        if token.text == '(':
            node = self.expr()
            self.take(')')
            return node
        raise SyntaxError('unexpected ' + token.text)


OPS = {
    '+': lambda a, b: a + b,
    '-': lambda a, b: a - b,
    '*': lambda a, b: a * b,
    '%': lambda a, b: a % b,
}


def evaluate(node, env):
    tag = node[0]
    if tag == 'num':
        return node[1]
    if tag == 'var':
        return env[node[1]]
    return OPS[node[1]](evaluate(node[2], env), evaluate(node[3], env))


def run(source):
    statements = Parser(tokenize(source)).program()
    env = {f'v{i}': i + 1 for i in range(20)}
    for _ in range(4):
        for name, node in statements:
            env[name] = evaluate(node, env)
    return len(statements), sum(env.values())


lines = []
for k in range(300):
    lines.append(f'v{k % 20} = (v{k * 7 % 20} + {k}) * v{k * 3 % 20} % 10007 - {k % 13};')
source = '\n'.join(lines)

total = 0
count = 0
for r in range(25):
    n, s = run(source.replace('10007', str(10007 + r * 2)))
    count += n
    total += s
print(count, total)
