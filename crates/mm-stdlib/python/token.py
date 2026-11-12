# token: CPython 3.14's token type constants (Lib/token.py, PSF license,
# see THIRD_PARTY.md), for tokenize.

__all__ = ['tok_name', 'ISTERMINAL', 'ISNONTERMINAL', 'ISEOF',
           'EXACT_TOKEN_TYPES']

ENDMARKER = 0
NAME = 1
NUMBER = 2
STRING = 3
NEWLINE = 4
INDENT = 5
DEDENT = 6
LPAR = 7
RPAR = 8
LSQB = 9
RSQB = 10
COLON = 11
COMMA = 12
SEMI = 13
PLUS = 14
MINUS = 15
STAR = 16
SLASH = 17
VBAR = 18
AMPER = 19
LESS = 20
GREATER = 21
EQUAL = 22
DOT = 23
PERCENT = 24
LBRACE = 25
RBRACE = 26
EQEQUAL = 27
NOTEQUAL = 28
LESSEQUAL = 29
GREATEREQUAL = 30
TILDE = 31
CIRCUMFLEX = 32
LEFTSHIFT = 33
RIGHTSHIFT = 34
DOUBLESTAR = 35
PLUSEQUAL = 36
MINEQUAL = 37
STAREQUAL = 38
SLASHEQUAL = 39
PERCENTEQUAL = 40
AMPEREQUAL = 41
VBAREQUAL = 42
CIRCUMFLEXEQUAL = 43
LEFTSHIFTEQUAL = 44
RIGHTSHIFTEQUAL = 45
DOUBLESTAREQUAL = 46
DOUBLESLASH = 47
DOUBLESLASHEQUAL = 48
AT = 49
ATEQUAL = 50
RARROW = 51
ELLIPSIS = 52
COLONEQUAL = 53
EXCLAMATION = 54
OP = 55
TYPE_IGNORE = 56
TYPE_COMMENT = 57
SOFT_KEYWORD = 58
FSTRING_START = 59
FSTRING_MIDDLE = 60
FSTRING_END = 61
TSTRING_START = 62
TSTRING_MIDDLE = 63
TSTRING_END = 64
COMMENT = 65
NL = 66
ERRORTOKEN = 67
ENCODING = 68
N_TOKENS = 69
NT_OFFSET = 256

tok_name = {}
for _name, _value in list(globals().items()):
    if isinstance(_value, int) and not _name.startswith('_'):
        tok_name[_value] = _name
__all__.extend(tok_name.values())

EXACT_TOKEN_TYPES = {
    '(': LPAR,
    ')': RPAR,
    '[': LSQB,
    ']': RSQB,
    ':': COLON,
    ',': COMMA,
    ';': SEMI,
    '+': PLUS,
    '-': MINUS,
    '*': STAR,
    '/': SLASH,
    '|': VBAR,
    '&': AMPER,
    '<': LESS,
    '>': GREATER,
    '=': EQUAL,
    '.': DOT,
    '%': PERCENT,
    '{': LBRACE,
    '}': RBRACE,
    '==': EQEQUAL,
    '!=': NOTEQUAL,
    '<=': LESSEQUAL,
    '>=': GREATEREQUAL,
    '~': TILDE,
    '^': CIRCUMFLEX,
    '<<': LEFTSHIFT,
    '>>': RIGHTSHIFT,
    '**': DOUBLESTAR,
    '+=': PLUSEQUAL,
    '-=': MINEQUAL,
    '*=': STAREQUAL,
    '/=': SLASHEQUAL,
    '%=': PERCENTEQUAL,
    '&=': AMPEREQUAL,
    '|=': VBAREQUAL,
    '^=': CIRCUMFLEXEQUAL,
    '<<=': LEFTSHIFTEQUAL,
    '>>=': RIGHTSHIFTEQUAL,
    '**=': DOUBLESTAREQUAL,
    '//': DOUBLESLASH,
    '//=': DOUBLESLASHEQUAL,
    '@': AT,
    '@=': ATEQUAL,
    '->': RARROW,
    '...': ELLIPSIS,
    ':=': COLONEQUAL,
    '!': EXCLAMATION,
}


def ISTERMINAL(x):
    return x < NT_OFFSET


def ISNONTERMINAL(x):
    return x >= NT_OFFSET


def ISEOF(x):
    return x == ENDMARKER
