"""Protocol primitives: event builders, JSON sanitising and a thread-safe writer.

Every event is a flat JSON object with an ``event`` and a ``ts`` key, written
as one line on the protocol stream. See ``docs/PROTOCOL.md``.
"""

from __future__ import annotations

import json
import math
import os
import platform
import threading
from collections.abc import Mapping
from datetime import datetime
from typing import IO, Any

from . import PROTOCOL_VERSION, __version__

# Error codes (``error.code``).
ALREADY_RUNNING = "already_running"
NOT_RUNNING = "not_running"
BAD_COMMAND = "bad_command"
TRACKER_INIT_FAILED = "tracker_init_failed"
INTERNAL = "internal"

ERROR_CODES = frozenset({ALREADY_RUNNING, NOT_RUNNING, BAD_COMMAND, TRACKER_INIT_FAILED, INTERNAL})


def now_iso() -> str:
    """Current local time, ISO-8601 with UTC offset, e.g. ``2026-11-02T20:50:26+01:00``."""
    return datetime.now().astimezone().isoformat(timespec="seconds")


def sanitize(value: Any) -> Any:
    """Return a JSON-safe copy of ``value``.

    NaN and +/-Infinity become ``None`` (they are not valid JSON), tuples and
    sets become lists, mapping keys become strings and anything else that JSON
    cannot represent (UUID, Path, ...) is converted with ``str``.
    """
    if value is None or isinstance(value, (bool, str, int)):
        return value
    if isinstance(value, float):
        return value if math.isfinite(value) else None
    if isinstance(value, Mapping):
        return {str(k): sanitize(v) for k, v in value.items()}
    if isinstance(value, (list, tuple, set, frozenset)):
        return [sanitize(v) for v in value]
    try:  # numpy scalars and other number-likes
        if hasattr(value, "item"):
            return sanitize(value.item())
    except Exception:  # noqa: BLE001 - best effort only
        pass
    return str(value)


def encode(event: Mapping[str, Any]) -> str:
    """Serialise one event as a single JSON line (without the trailing newline)."""
    return json.dumps(sanitize(event), allow_nan=False, ensure_ascii=True, separators=(",", ":"))


def make_event(name: str, **fields: Any) -> dict[str, Any]:
    """Build an event dict: ``{"event": name, "ts": now, **fields}``."""
    event: dict[str, Any] = {"event": name, "ts": now_iso()}
    event.update(fields)
    return event


def codecarbon_version() -> str | None:
    """Installed CodeCarbon version, without importing the (slow) package if possible."""
    try:
        from importlib.metadata import version

        return version("codecarbon")
    except Exception:  # noqa: BLE001
        pass
    try:
        from codecarbon import __version__ as cc_version

        return str(cc_version)
    except Exception:  # noqa: BLE001
        return None


def ready_event() -> dict[str, Any]:
    return make_event(
        "ready",
        protocol_version=PROTOCOL_VERSION,
        agent_version=__version__,
        codecarbon_version=codecarbon_version(),
        python_version=platform.python_version(),
        pid=os.getpid(),
        platform=platform.platform(),
    )


def started_event(payload: Mapping[str, Any]) -> dict[str, Any]:
    """``payload`` is what :meth:`TrackerSession.start` returns."""
    return make_event("started", **payload)


def measurement_event(payload: Mapping[str, Any]) -> dict[str, Any]:
    """``payload`` is what :meth:`TrackerSession.sample` returns."""
    return make_event("measurement", **payload)


def stopped_event(summary: Mapping[str, Any]) -> dict[str, Any]:
    return make_event("stopped", summary=dict(summary))


def status_event(state: str, last_measurement: Mapping[str, Any] | None) -> dict[str, Any]:
    return make_event(
        "status",
        state=state,
        last_measurement=dict(last_measurement) if last_measurement is not None else None,
    )


def log_event(level: str, message: str) -> dict[str, Any]:
    return make_event("log", level=level, message=message)


def error_event(code: str, message: str) -> dict[str, Any]:
    return make_event("error", code=code, message=message)


class Writer:
    """Writes protocol events, one JSON object per line, from any thread.

    Write failures (closed pipe: the app went away) never raise; they set
    :attr:`broken` so the runner can shut down cleanly.
    """

    def __init__(self, stream: IO[str]) -> None:
        self._stream = stream
        self._lock = threading.Lock()
        self.broken = False

    def write(self, event: Mapping[str, Any]) -> bool:
        """Write ``event``; return ``False`` if the stream is broken."""
        line = encode(event) + "\n"
        with self._lock:
            if self.broken:
                return False
            try:
                self._stream.write(line)
                self._stream.flush()
            except (OSError, ValueError):  # BrokenPipeError, write to closed file
                self.broken = True
                return False
        return True
