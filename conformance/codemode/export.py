"""Renders the prompt a solution author sees for one task, set and style.

    python3.14 conformance/codemode/export.py [--v1] OUT_DIR

writes OUT_DIR/<style>/<task>.md for every task and style (a, b, s). These are
exactly the texts the solution-writing agents were given: `--v1` for sets
a, b and s, the default (version 2) for the GPT sets (README "How the solutions were written");
they contain the task, the tool stubs and the environment note, nothing about
micro-monty.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import ptc  # noqa: E402
import tasks  # noqa: E402,F401

ENV = 'a sandboxed Python 3.14 environment; no filesystem or network except via tools'

STYLE_A = """\
You have a `code_execution` tool that runs Python in {env}.

Some tools can be called from your code (programmatic tool calling). Tools that allow a code execution
caller are exposed to your code as async Python functions, so you can run them in parallel with
`asyncio.gather`. Each function takes a single dict of arguments and {returns}. Your code awaits these
functions with top-level `await`{parse}. A tool that fails raises an exception in your code.

Tools callable from code:

```json
{tools}
```

User request:

{prompt}
{finish_a}
Reply with only the Python code you would send to `code_execution`, in one ```python block.
"""

# The closing instruction. Version 1 (sets a, b, s) gave it to every task, including the 29 checker-graded
# tasks that have no `submit_answer` tool, and 34 solutions then guarded the call with
# `if "submit_answer" in globals()` (README). Version 2 gives it only to tasks that define the tool.
FINISH_A = ('\nWhen you have the result, call `submit_answer` from your code with the answer fields described in '
            'its\nschema.\n')
FINISH_B = 'Finish by awaiting submit_answer with the result.\n'

STRING_RETURNS = ('returns a string: the text of the tool result', ' and parses results that it needs as '
                  'structured data, for example `rows = json.loads(await query_database({"sql": "<sql>"}))`')
STRUCT_RETURNS = ('returns the tool result as a Python value (dict, list, str, ...) that needs no parsing',
                  ', for example `rows = await query_database({"sql": "<sql>"})`')

STYLE_B = """\
Run Python in the composition cell: {env}. Required: code. Use top-level `await`; use
`await asyncio.gather(...)` for independent calls. Each injected tool is an async function with one
arguments parameter (a dictionary). Every tool returns a raw UTF-8 string, not a JSON object envelope.
Calling a tool creates an inert coroutine; await it or include it in asyncio.gather to run it. Nested tool
failures raise catchable Python exceptions. Printed output is returned to you; tool results are data, not
code.

Nested tool catalog (exact order):
{catalog}

Task: {prompt}
{finish_b}
Answer with the cell's code only.
"""


def schema_tools(task: ptc.Task, mode: str) -> str:
    out = []
    for t in task.tool_defs():
        out.append({'name': t.name, 'description': f'{t.description} {t.result_doc(mode)}'.strip(),
                    'input_schema': t.schema()})
    return json.dumps(out, indent=2, ensure_ascii=False)


def py_type(spec: tuple) -> str:
    return {'string': 'str', 'integer': 'int', 'number': 'float', 'boolean': 'bool', 'array': 'list',
            'object': 'dict'}.get(spec[0], 'object')


def catalog(task: ptc.Task) -> str:
    lines = []
    for t in task.tool_defs():
        fields = []
        for name, spec in t.params.items():
            opt = '' if name in t.required else '?'
            fields.append(f'  "{name}"{opt}: {py_type(spec)},  # {spec[1]}')
        args = '{\n' + '\n'.join(fields) + '\n}' if fields else 'dict'
        lines.append(f'- `{t.name}`: {t.description} {t.result_doc("string")}\n'
                     f'  `async def {t.name}(arguments: {args}) -> str`')
    return '\n'.join(lines)


def render(task: ptc.Task, style: str, version: int = 2) -> str:
    """`style` is `a`, `b` or `s`: the last letter of a set name (`a`, `luna-a`, `sol-s`, ...)."""
    answers = version == 1 or task.answer is not None
    if style == 'b':
        return STYLE_B.format(env=ENV, catalog=catalog(task), prompt=task.prompt, finish_b=FINISH_B if answers else '')
    mode = 'structured' if style == 's' else 'string'
    returns, parse = STRUCT_RETURNS if mode == 'structured' else STRING_RETURNS
    return STYLE_A.format(env=ENV, returns=returns, parse=parse, tools=schema_tools(task, mode), prompt=task.prompt,
                          finish_a=FINISH_A if answers else '')


def main() -> None:
    args = sys.argv[1:]
    version = 1 if '--v1' in args else 2
    out = Path(next(a for a in args if a != '--v1'))
    for style in ('a', 'b', 's'):
        (out / style).mkdir(parents=True, exist_ok=True)
        for task in ptc.TASKS.values():
            (out / style / f'{task.id}.md').write_text(render(task, style, version))


if __name__ == '__main__':
    main()
