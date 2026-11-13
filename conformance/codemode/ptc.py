"""Core of the codemode workload: tools, tasks, the value codec and checkers.

A task names the mocked tools a solution may call, a prompt and a checker.
Tools are plain Python functions over a deterministic world (fixture data
built from fixed seeds); `ToolHost` answers one run's calls for either
engine, so CPython and micro-monty see byte-identical tool behaviour.

Two result modes:

- `string`: the programmatic tool calling convention. Every tool returns a
  `str` (JSON text, or raw text for text tools) that code parses with
  `json.loads`.
- `structured`: tools return the parsed value (dict, list, ...).
"""

from __future__ import annotations

import json
import math
from dataclasses import dataclass, field
from typing import Any, Callable

# --------------------------------------------------------------------------
# Value codec shared with the runner (see runner/src/main.rs)


def encode(value: Any) -> Any:
    """A Python value in the tagged JSON form."""
    if value is None or isinstance(value, (bool, str)):
        return value
    if isinstance(value, int):
        return value if -(2**63) <= value < 2**63 else {'$i': str(value)}
    if isinstance(value, float):
        return {'$f': repr(value)}
    if isinstance(value, list):
        return [encode(v) for v in value]
    if isinstance(value, tuple):
        return {'$t': [encode(v) for v in value]}
    if isinstance(value, dict):
        return {'$d': [[k, encode(v)] for k, v in value.items()]}
    if isinstance(value, (bytes, bytearray)):
        return {'$b': bytes(value).hex()}
    raise TypeError(f'Object of type {type(value).__name__} is not JSON serializable')


class Unconverted:
    """A guest value the runner could not convert (lenient mode only)."""

    def __init__(self, text: str):
        self.text = text

    def __repr__(self) -> str:
        return f'<unconverted {self.text}>'


def decode(value: Any) -> Any:
    """A tagged JSON value as Python."""
    if isinstance(value, list):
        return [decode(v) for v in value]
    if isinstance(value, dict):
        if '$f' in value:
            return float(value['$f'])
        if '$i' in value:
            return int(value['$i'])
        if '$t' in value:
            return tuple(decode(v) for v in value['$t'])
        if '$d' in value:
            return {k: decode(v) for k, v in value['$d']}
        if '$b' in value:
            return bytes.fromhex(value['$b'])
        if '$r' in value:
            return tuple(decode(v) for _, v in value['fields'])
        if '$u' in value:
            return Unconverted(value['$u'])
    return value


# --------------------------------------------------------------------------
# Tools


class ToolError(Exception):
    """A tool failure; the calling code sees `kind(message)` raised."""

    def __init__(self, message: str, kind: str = 'RuntimeError'):
        super().__init__(message)
        self.kind = kind
        self.message = message


@dataclass
class Tool:
    name: str
    description: str
    # JSON-schema properties: name -> (type, description[, extra schema]).
    params: dict[str, tuple]
    required: list[str]
    # What the result holds, independent of the mode ("an object {...}").
    returns: str
    impl: Callable[..., Any]
    # Text tools return raw text in both modes (file contents, log text).
    text: bool = False

    def schema(self) -> dict:
        props = {}
        for pname, spec in self.params.items():
            ptype, pdesc, *extra = spec
            prop = {'type': ptype, 'description': pdesc}
            if extra:
                prop.update(extra[0])
            props[pname] = prop
        return {'type': 'object', 'properties': props, 'required': list(self.required)}

    def result_doc(self, mode: str) -> str:
        if self.text:
            return f'Returns {self.returns}.'
        if mode == 'string':
            return f'Returns a JSON string encoding {self.returns}.'
        return f'Returns {self.returns}, as a Python value (already parsed).'


TOOLS: dict[str, Tool] = {}


def tool(name: str, description: str, params: dict, returns: str, required=None, text=False):
    """Registers a tool implementation `impl(args, state)`."""

    def wrap(impl):
        req = list(params) if required is None else required
        TOOLS[name] = Tool(name, description, params, req, returns, impl, text)
        return impl

    return wrap


# --------------------------------------------------------------------------
# Tasks


@dataclass
class Task:
    id: str
    category: str
    prompt: str
    tools: list[str]
    # Answer fields of the task's `submit_answer` tool: name -> (JSON type,
    # description), or a bare description for a string.
    answer: dict[str, str] | None
    check: Callable[['Transcript'], str | None]
    # Per-task tool behaviour (failure injection etc.), read by tool impls.
    config: dict = field(default_factory=dict)

    def tool_defs(self) -> list[Tool]:
        defs = [TOOLS[t] for t in self.tools]
        if self.answer is not None:
            props = {k: v if isinstance(v, tuple) else ('string', v) for k, v in self.answer.items()}
            defs.append(
                Tool(
                    'submit_answer',
                    'Submit the final answer of this task. Call it once, at the end.',
                    props,
                    list(self.answer),
                    'the string "ok"',
                    lambda args, state: 'ok',
                    text=True,
                )
            )
        return defs


TASKS: dict[str, Task] = {}


def task(id, category, prompt, tools, *, answer=None, expect=None, check=None, config=None, tol=1e-6, unordered=()):
    """Registers a task. `expect` is the expected `submit_answer` input (a
    dict, compared with `tol` float tolerance; fields in `unordered` compare
    as multisets); `check` is a custom checker over the transcript."""
    if id in TASKS:
        raise ValueError(f'duplicate task {id}')
    if check is None:
        expected = expect

        def check(t: 'Transcript') -> str | None:
            got = t.answer()
            if got is None:
                return 'submit_answer was not called'
            return compare(expected, got, tol, unordered)

    TASKS[id] = Task(id, category, prompt.strip(), tools, answer, check, config or {})
    return TASKS[id]


# --------------------------------------------------------------------------
# Runs


@dataclass
class Call:
    name: str
    args: Any
    result: Any = None
    error: tuple[str, str] | None = None


@dataclass
class Transcript:
    calls: list[Call] = field(default_factory=list)
    stdout: str = ''
    stderr: str = ''
    error: dict | None = None

    def answer(self) -> Any:
        subs = [c for c in self.calls if c.name == 'submit_answer' and c.error is None]
        return subs[-1].args if subs else None

    def to(self, name: str) -> list[Any]:
        return [c.args for c in self.calls if c.name == name and c.error is None]


class ToolHost:
    """Answers one run's tool calls."""

    def __init__(self, task: Task, mode: str):
        self.task = task
        self.mode = mode
        self.tools = {t.name: t for t in task.tool_defs()}
        self.state: dict = {'config': task.config, 'counts': {}}
        self.transcript = Transcript()

    def handle(self, name: str, args: list, kwargs: list) -> dict:
        """One call in the wire form; returns the wire reply."""
        tool = self.tools.get(name)
        if kwargs or len(args) != 1:
            return self._fail(name, None, 'TypeError', f'{name}() takes one dict argument')
        try:
            arguments = json.loads(json.dumps(args[0], allow_nan=False))
        except (TypeError, ValueError) as e:
            return self._fail(name, None, 'TypeError', str(e))
        if not isinstance(arguments, dict):
            return self._fail(name, arguments, 'TypeError', f'{name}() argument must be a dict')
        if tool is None:
            return self._fail(name, arguments, 'NameError', f'no tool {name}')
        missing = [p for p in tool.required if p not in arguments]
        if missing:
            return self._fail(name, arguments, 'ValueError', f'{name}: missing required argument {missing[0]!r}')
        counts = self.state['counts']
        counts[name] = counts.get(name, 0) + 1
        try:
            value = tool.impl(arguments, self.state)
        except ToolError as e:
            return self._fail(name, arguments, e.kind, e.message)
        if self.mode == 'string' and not isinstance(value, str):
            value = json.dumps(value)
        self.transcript.calls.append(Call(name, arguments, value))
        return {'ok': encode(value)}

    def _fail(self, name, arguments, kind, message) -> dict:
        self.transcript.calls.append(Call(name, arguments, None, (kind, message)))
        return {'err': [kind, message]}


# --------------------------------------------------------------------------
# Checking


def compare(want: Any, got: Any, tol: float, unordered=(), path: str = '') -> str | None:
    """None if `got` matches `want`, else where they differ."""
    where = path or 'answer'
    if isinstance(want, bool) or isinstance(got, bool):
        return None if want is got else f'{where}: expected {want!r}, got {got!r}'
    if isinstance(want, (int, float)) and isinstance(got, (int, float)):
        if math.isclose(want, got, rel_tol=1e-9, abs_tol=tol):
            return None
        return f'{where}: expected {want!r}, got {got!r}'
    if isinstance(want, dict):
        if not isinstance(got, dict):
            return f'{where}: expected an object, got {type(got).__name__}'
        if set(want) != set(got):
            return f'{where}: expected keys {sorted(want)}, got {sorted(map(str, got))}'
        for k in want:
            sub = f'{path}.{k}' if path else k
            r = compare(want[k], got[k], tol, unordered, sub)
            if r:
                return r
        return None
    if isinstance(want, list):
        if not isinstance(got, list):
            return f'{where}: expected a list, got {type(got).__name__}'
        if len(want) != len(got):
            return f'{where}: expected {len(want)} items, got {len(got)}'
        if path in unordered:
            want = sorted(want, key=_key)
            got = sorted(got, key=_key)
        for i, (w, g) in enumerate(zip(want, got)):
            r = compare(w, g, tol, unordered, f'{path}[{i}]')
            if r:
                return r
        return None
    if want != got:
        return f'{where}: expected {want!r}, got {got!r}'
    return None


def _key(v: Any) -> str:
    return json.dumps(v, sort_keys=True)


def r2(x: float) -> float:
    return round(x, 2)
