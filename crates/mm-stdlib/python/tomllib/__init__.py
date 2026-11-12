# Adapted from CPython 3.14.7's Lib/tomllib (MIT, Taneli Hukkinen; PSF, see
# THIRD_PARTY.md): relative imports made absolute (micro-monty has none);
# MAX_KEY_PARTS fixed at 1000 (_parser.py).
# SPDX-License-Identifier: MIT
# SPDX-FileCopyrightText: 2021 Taneli Hukkinen
# Licensed to PSF under a Contributor Agreement.

__all__ = ("loads", "load", "TOMLDecodeError")

from tomllib._parser import TOMLDecodeError, load, loads

# Pretend this exception was created here.
TOMLDecodeError.__module__ = __name__
