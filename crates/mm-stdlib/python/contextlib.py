# contextlib: CPython 3.14's Lib/contextlib.py, trimmed to what
# micro-monty can run. Left out: chdir (no filesystem) and the
# exception-group branch of suppress (no ExceptionGroup).

import abc
import sys
from collections import deque
from functools import wraps

__all__ = ['asynccontextmanager', 'contextmanager', 'closing', 'nullcontext',
           'AbstractContextManager', 'AbstractAsyncContextManager',
           'AsyncExitStack', 'ContextDecorator', 'ExitStack',
           'redirect_stdout', 'redirect_stderr', 'suppress', 'aclosing']


def _check_methods(C, *methods):
    mro = C.__mro__
    for method in methods:
        for B in mro:
            # Builtin types have no `__dict__` here, nor these methods.
            d = getattr(B, '__dict__', None)
            if d is not None and method in d:
                if d[method] is None:
                    return NotImplemented
                break
        else:
            return NotImplemented
    return True


class AbstractContextManager(abc.ABC):
    """An abstract base class for context managers."""

    def __enter__(self):
        """Return `self` upon entering the runtime context."""
        return self

    @abc.abstractmethod
    def __exit__(self, exc_type, exc_value, traceback):
        """Raise any exception triggered within the runtime context."""
        return None

    @classmethod
    def __subclasshook__(cls, C):
        if cls is AbstractContextManager:
            return _check_methods(C, '__enter__', '__exit__')
        return NotImplemented


class AbstractAsyncContextManager(abc.ABC):
    """An abstract base class for asynchronous context managers."""

    async def __aenter__(self):
        """Return `self` upon entering the runtime context."""
        return self

    @abc.abstractmethod
    async def __aexit__(self, exc_type, exc_value, traceback):
        """Raise any exception triggered within the runtime context."""
        return None

    @classmethod
    def __subclasshook__(cls, C):
        if cls is AbstractAsyncContextManager:
            return _check_methods(C, '__aenter__', '__aexit__')
        return NotImplemented


class ContextDecorator(object):
    "A base class or mixin that enables context managers to work as decorators."

    def _recreate_cm(self):
        return self

    def __call__(self, func):
        @wraps(func)
        def inner(*args, **kwds):
            with self._recreate_cm():
                return func(*args, **kwds)
        return inner


class AsyncContextDecorator(object):
    "A base class or mixin that enables async context managers to work as decorators."

    def _recreate_cm(self):
        return self

    def __call__(self, func):
        @wraps(func)
        async def inner(*args, **kwds):
            async with self._recreate_cm():
                return await func(*args, **kwds)
        return inner


class _GeneratorContextManager(AbstractContextManager, ContextDecorator):
    """Helper for @contextmanager decorator."""

    def __init__(self, func, args, kwds):
        self.gen = func(*args, **kwds)
        self.func, self.args, self.kwds = func, args, kwds

    def _recreate_cm(self):
        return self.__class__(self.func, self.args, self.kwds)

    def __enter__(self):
        del self.args, self.kwds, self.func
        try:
            return next(self.gen)
        except StopIteration:
            raise RuntimeError("generator didn't yield") from None

    def __exit__(self, typ, value, traceback):
        if typ is None:
            try:
                next(self.gen)
            except StopIteration:
                return False
            else:
                try:
                    raise RuntimeError("generator didn't stop")
                finally:
                    self.gen.close()
        else:
            if value is None:
                value = typ()
            try:
                self.gen.throw(value)
            except StopIteration as exc:
                return exc is not value
            except RuntimeError as exc:
                if exc is value:
                    exc.__traceback__ = traceback
                    return False
                if isinstance(value, StopIteration) and exc.__cause__ is value:
                    value.__traceback__ = traceback
                    return False
                raise
            except BaseException as exc:
                if exc is not value:
                    raise
                exc.__traceback__ = traceback
                return False
            try:
                raise RuntimeError("generator didn't stop after throw()")
            finally:
                self.gen.close()


class _AsyncGeneratorContextManager(AbstractAsyncContextManager, AsyncContextDecorator):
    """Helper for @asynccontextmanager decorator."""

    def __init__(self, func, args, kwds):
        self.gen = func(*args, **kwds)
        self.func, self.args, self.kwds = func, args, kwds

    def _recreate_cm(self):
        return self.__class__(self.func, self.args, self.kwds)

    async def __aenter__(self):
        del self.args, self.kwds, self.func
        try:
            return await anext(self.gen)
        except StopAsyncIteration:
            raise RuntimeError("generator didn't yield") from None

    async def __aexit__(self, typ, value, traceback):
        if typ is None:
            try:
                await anext(self.gen)
            except StopAsyncIteration:
                return False
            else:
                try:
                    raise RuntimeError("generator didn't stop")
                finally:
                    await self.gen.aclose()
        else:
            if value is None:
                value = typ()
            try:
                await self.gen.athrow(value)
            except StopAsyncIteration as exc:
                return exc is not value
            except RuntimeError as exc:
                if exc is value:
                    exc.__traceback__ = traceback
                    return False
                if (
                    isinstance(value, (StopIteration, StopAsyncIteration))
                    and exc.__cause__ is value
                ):
                    value.__traceback__ = traceback
                    return False
                raise
            except BaseException as exc:
                if exc is not value:
                    raise
                exc.__traceback__ = traceback
                return False
            try:
                raise RuntimeError("generator didn't stop after athrow()")
            finally:
                await self.gen.aclose()


def contextmanager(func):
    """@contextmanager decorator."""
    @wraps(func)
    def helper(*args, **kwds):
        return _GeneratorContextManager(func, args, kwds)
    return helper


def asynccontextmanager(func):
    """@asynccontextmanager decorator."""
    @wraps(func)
    def helper(*args, **kwds):
        return _AsyncGeneratorContextManager(func, args, kwds)
    return helper


class closing(AbstractContextManager):
    """Context to automatically close something at the end of a block."""

    def __init__(self, thing):
        self.thing = thing

    def __enter__(self):
        return self.thing

    def __exit__(self, *exc_info):
        self.thing.close()


class aclosing(AbstractAsyncContextManager):
    """Async context manager for safely finalizing an asynchronously cleaned-up
    resource such as an async generator, calling its ``aclose()`` method."""

    def __init__(self, thing):
        self.thing = thing

    async def __aenter__(self):
        return self.thing

    async def __aexit__(self, *exc_info):
        await self.thing.aclose()


class _RedirectStream(AbstractContextManager):

    _stream = None

    def __init__(self, new_target):
        self._new_target = new_target
        self._old_targets = []

    def __enter__(self):
        self._old_targets.append(getattr(sys, self._stream))
        setattr(sys, self._stream, self._new_target)
        return self._new_target

    def __exit__(self, exctype, excinst, exctb):
        setattr(sys, self._stream, self._old_targets.pop())


class redirect_stdout(_RedirectStream):
    """Context manager for temporarily redirecting stdout to another file."""

    _stream = 'stdout'


class redirect_stderr(_RedirectStream):
    """Context manager for temporarily redirecting stderr to another file."""

    _stream = 'stderr'


class suppress(AbstractContextManager):
    """Context manager to suppress specified exceptions."""

    def __init__(self, *exceptions):
        self._exceptions = exceptions

    def __enter__(self):
        pass

    def __exit__(self, exctype, excinst, exctb):
        if exctype is None:
            return
        if issubclass(exctype, self._exceptions):
            return True
        return False


class _BaseExitStack:
    """A base class for ExitStack and AsyncExitStack."""

    @staticmethod
    def _create_exit_wrapper(cm, cm_exit):
        def _exit_wrapper(exc_type, exc, tb):
            return cm_exit(cm, exc_type, exc, tb)
        return _exit_wrapper

    @staticmethod
    def _create_cb_wrapper(callback, /, *args, **kwds):
        def _exit_wrapper(exc_type, exc, tb):
            callback(*args, **kwds)
        return _exit_wrapper

    def __init__(self):
        self._exit_callbacks = deque()

    def pop_all(self):
        """Preserve the context stack by transferring it to a new instance."""
        new_stack = type(self)()
        new_stack._exit_callbacks = self._exit_callbacks
        self._exit_callbacks = deque()
        return new_stack

    def push(self, exit):
        """Registers a callback with the standard __exit__ method signature."""
        _cb_type = type(exit)
        try:
            exit_method = _cb_type.__exit__
        except AttributeError:
            self._push_exit_callback(exit)
        else:
            self._push_cm_exit(exit, exit_method)
        return exit

    def enter_context(self, cm):
        """Enters the supplied context manager."""
        cls = type(cm)
        try:
            _enter = cls.__enter__
            _exit = cls.__exit__
        except AttributeError:
            raise TypeError(f"'{cls.__module__}.{cls.__qualname__}' object does "
                            f"not support the context manager protocol") from None
        result = _enter(cm)
        self._push_cm_exit(cm, _exit)
        return result

    def callback(self, callback, /, *args, **kwds):
        """Registers an arbitrary callback and arguments."""
        _exit_wrapper = self._create_cb_wrapper(callback, *args, **kwds)
        _exit_wrapper.__wrapped__ = callback
        self._push_exit_callback(_exit_wrapper)
        return callback

    def _push_cm_exit(self, cm, cm_exit):
        _exit_wrapper = self._create_exit_wrapper(cm, cm_exit)
        self._push_exit_callback(_exit_wrapper, True)

    def _push_exit_callback(self, callback, is_sync=True):
        self._exit_callbacks.append((is_sync, callback))


def _fix_exception_context(new_exc, old_exc, frame_exc):
    while 1:
        exc_context = new_exc.__context__
        if exc_context is None or exc_context is old_exc:
            return
        if exc_context is frame_exc:
            break
        new_exc = exc_context
    new_exc.__context__ = old_exc


class ExitStack(_BaseExitStack, AbstractContextManager):
    """Context manager for dynamic management of a stack of exit callbacks."""

    def __enter__(self):
        return self

    def __exit__(self, *exc_details):
        exc = exc_details[1]
        received_exc = exc is not None
        frame_exc = sys.exception()
        suppressed_exc = False
        pending_raise = False
        while self._exit_callbacks:
            is_sync, cb = self._exit_callbacks.pop()
            assert is_sync
            try:
                if exc is None:
                    exc_details = None, None, None
                else:
                    exc_details = type(exc), exc, exc.__traceback__
                if cb(*exc_details):
                    suppressed_exc = True
                    pending_raise = False
                    exc = None
            except BaseException as new_exc:
                _fix_exception_context(new_exc, exc, frame_exc)
                pending_raise = True
                exc = new_exc
        if pending_raise:
            try:
                fixed_ctx = exc.__context__
                raise exc
            except BaseException:
                exc.__context__ = fixed_ctx
                raise
        return received_exc and suppressed_exc

    def close(self):
        """Immediately unwind the context stack."""
        self.__exit__(None, None, None)


class AsyncExitStack(_BaseExitStack, AbstractAsyncContextManager):
    """Async context manager for dynamic management of a stack of exit
    callbacks."""

    @staticmethod
    def _create_async_exit_wrapper(cm, cm_exit):
        def _exit_wrapper(exc_type, exc, tb):
            return cm_exit(cm, exc_type, exc, tb)
        return _exit_wrapper

    @staticmethod
    def _create_async_cb_wrapper(callback, /, *args, **kwds):
        async def _exit_wrapper(exc_type, exc, tb):
            await callback(*args, **kwds)
        return _exit_wrapper

    async def enter_async_context(self, cm):
        """Enters the supplied async context manager."""
        cls = type(cm)
        try:
            _enter = cls.__aenter__
            _exit = cls.__aexit__
        except AttributeError:
            raise TypeError(f"'{cls.__module__}.{cls.__qualname__}' object does "
                            f"not support the asynchronous context manager protocol"
                            ) from None
        result = await _enter(cm)
        self._push_async_cm_exit(cm, _exit)
        return result

    def push_async_exit(self, exit):
        """Registers a coroutine function with the standard __aexit__ method
        signature."""
        _cb_type = type(exit)
        try:
            exit_method = _cb_type.__aexit__
        except AttributeError:
            self._push_exit_callback(exit, False)
        else:
            self._push_async_cm_exit(exit, exit_method)
        return exit

    def push_async_callback(self, callback, /, *args, **kwds):
        """Registers an arbitrary coroutine function and arguments."""
        _exit_wrapper = self._create_async_cb_wrapper(callback, *args, **kwds)
        _exit_wrapper.__wrapped__ = callback
        self._push_exit_callback(_exit_wrapper, False)
        return callback

    async def aclose(self):
        """Immediately unwind the context stack."""
        await self.__aexit__(None, None, None)

    def _push_async_cm_exit(self, cm, cm_exit):
        _exit_wrapper = self._create_async_exit_wrapper(cm, cm_exit)
        self._push_exit_callback(_exit_wrapper, False)

    async def __aenter__(self):
        return self

    async def __aexit__(self, *exc_details):
        exc = exc_details[1]
        received_exc = exc is not None
        frame_exc = sys.exception()
        suppressed_exc = False
        pending_raise = False
        while self._exit_callbacks:
            is_sync, cb = self._exit_callbacks.pop()
            try:
                if exc is None:
                    cb_details = None, None, None
                else:
                    cb_details = type(exc), exc, exc.__traceback__
                if is_sync:
                    cb_suppress = cb(*cb_details)
                else:
                    cb_suppress = await cb(*cb_details)
                if cb_suppress:
                    suppressed_exc = True
                    pending_raise = False
                    exc = None
            except BaseException as new_exc:
                _fix_exception_context(new_exc, exc, frame_exc)
                pending_raise = True
                exc = new_exc
        if pending_raise:
            try:
                fixed_ctx = exc.__context__
                raise exc
            except BaseException:
                exc.__context__ = fixed_ctx
                raise
        return received_exc and suppressed_exc


class nullcontext(AbstractContextManager, AbstractAsyncContextManager):
    """Context manager that does no additional processing."""

    def __init__(self, enter_result=None):
        self.enter_result = enter_result

    def __enter__(self):
        return self.enter_result

    def __exit__(self, *excinfo):
        pass

    async def __aenter__(self):
        return self.enter_result

    async def __aexit__(self, *excinfo):
        pass
