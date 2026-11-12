# Adapted from CPython 3.14.7's Lib/tomllib (MIT, Taneli Hukkinen; PSF, see
# THIRD_PARTY.md): relative imports made absolute (micro-monty has none).
# SPDX-License-Identifier: MIT
# SPDX-FileCopyrightText: 2021 Taneli Hukkinen
# Licensed to PSF under a Contributor Agreement.

from typing import Any, Callable, Tuple

# Type annotations
ParseFloat = Callable[[str], Any]
Key = Tuple[str, ...]
Pos = int
