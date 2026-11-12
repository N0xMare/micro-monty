# contextvars: Token, Context.run and copy_context; each task runs in a
# copy of its creator's context, or the one passed to create_task(context=)
# (TaskGroup too); call_soon copies the context; asyncio.run runs in a copy.
# SystemExit.code; KeyboardInterrupt and SystemExit raised in a task leave
# asyncio.run (cancelling main first), and a loop callback's SystemExit
# too; a future whose exception nobody retrieved is reported.
import contextvars, asyncio
v = contextvars.ContextVar('v', default=0)
w = contextvars.ContextVar('w')
t = v.set(1)
print(repr(t).split(' at ')[0], t.var is v, t.old_value, t.old_value is contextvars.Token.MISSING)
v.reset(t)
try: v.reset(t)
except RuntimeError as e: print(str(e).split(' at ')[0])
try: w.get()
except LookupError as e: print('LE', type(e).__name__, str(e).split(' at ')[0])
ctx = contextvars.copy_context()
print(type(ctx).__name__, len(ctx), v in ctx, list(ctx.items()))
def f(x):
    v.set(x); return v.get()
print(ctx.run(f, 5), v.get(), ctx[v], ctx.get(w, 'd'), len(ctx))
try: ctx.run(ctx.run, f, 1)
except RuntimeError as e: print(str(e).split(' at ')[0])
t2 = ctx.run(v.set, 9)
try: v.reset(t2)
except ValueError as e: print(str(e).split(' at ')[0])
try: ctx[1]
except TypeError as e: print(e)
c2 = contextvars.Context(); print(len(c2), c2 == contextvars.Context(), ctx.copy() == ctx)
with v.set(3) as tk: print('in', v.get())
print('out', v.get())
async def child(n):
    v.set(n); await asyncio.sleep(0); return v.get()
async def main():
    v.set(100)
    r = await asyncio.gather(child(1), child(2))
    print(r, v.get())
    c = contextvars.copy_context()
    t = asyncio.create_task(child(7), context=c)
    print(await t, c[v], v.get())
    loop = asyncio.get_running_loop()
    loop.call_soon(lambda: print('cb sees', v.get()))
    v.set(101)
    await asyncio.sleep(0)
    async with asyncio.TaskGroup() as tg:
        tt = tg.create_task(child(8), context=c)
    print(tt.result(), c[v])
asyncio.run(main())
print('after run', v.get())
import asyncio
async def child(e):
    await asyncio.sleep(0.01)
    raise e
keep = []
async def main1():
    t = asyncio.create_task(child(SystemExit(4)))
    keep.append(t)
    try:
        await asyncio.sleep(1)
    except asyncio.CancelledError:
        print('main1 cancelled', asyncio.current_task().cancelling())
        raise
    print('main end')
async def main2():
    t = asyncio.create_task(child(KeyboardInterrupt()))
    try:
        await t
    except KeyboardInterrupt:
        print('caught KI in main')
    except asyncio.CancelledError as e:
        print('main2 cancelled', repr(e))
    print('main2 end')
async def other():
    try:
        await asyncio.sleep(5)
    finally:
        print('other cleanup')
async def main3():
    asyncio.create_task(other())
    asyncio.get_running_loop().call_soon(lambda: (_ for _ in ()).throw(SystemExit(7)))
    await asyncio.sleep(1)
for m in (main1, main2, main3):
    try:
        asyncio.run(m())
    except BaseException as e:
        print('out of run', type(e).__name__, e.args, getattr(e, 'code', '-'), repr(e.__context__))
print(keep[0].exception())
print(SystemExit().code, SystemExit(1, 2).code, SystemExit('x').code)
e = SystemExit(5); e.code = 9; print(e.code, e.args)
async def fail(): raise ValueError('lost')
async def main4():
    f = asyncio.get_running_loop().create_future()
    f.set_exception(KeyError('kf'))
    t2 = asyncio.create_task(fail())
    await asyncio.sleep(0.01)
    t2.exception()
    print('main4 end')
asyncio.run(main4())
print('after 4')
