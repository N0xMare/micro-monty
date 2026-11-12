"""The codemode tasks: importing this package registers every task in `ptc.TASKS`."""

from tasks import worlds  # noqa: F401  (tools first)
from tasks import (aggregation, dates, encoding, logs, negative, paging, parallel, regex,  # noqa: F401
                   reports, reshape, stats, workflow)
