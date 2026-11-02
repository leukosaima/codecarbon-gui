"""The protocol loop: commands from stdin, events to the writer.

* The main thread reads command lines from a queue fed by a reader thread,
  so signals (SIGTERM / SIGINT) are noticed promptly even while stdin blocks.
* While a tracker runs, a sampling thread emits a ``measurement`` every
  ``interval_secs`` (on a fixed schedule, independent of the stdin loop).
* No exception ever kills the loop: it is reported as ``error(internal)``.
"""

from __future__ import annotations

import contextlib
import json
import logging
import os
import queue
import signal
import sys
import threading
import time
import traceback
from collections.abc import Callable, Mapping
from typing import Any, Protocol

from .protocol import (
    ALREADY_RUNNING,
    BAD_COMMAND,
    INTERNAL,
    NOT_RUNNING,
    TRACKER_INIT_FAILED,
    Writer,
    error_event,
    log_event,
    measurement_event,
    ready_event,
    started_event,
    status_event,
    stopped_event,
)
from .tracker import DEFAULT_LOG_LEVEL, StartError, TrackerSession

FORWARDED_LOGGERS = ("codecarbon", "codecarbon_agent")

# Warnings that are noise for a GUI user (the agent causes them on purpose,
# or they only frame another record that is forwarded anyway).
IGNORED_LOG_PREFIXES = (
    "Multiple instances of codecarbon are allowed",
    "graceful shutdown. Exceptions:",
    "stopping.",
)


class LineSource(Protocol):
    def readline(self) -> str: ...


def format_record(record: logging.LogRecord) -> str:
    """Message of ``record`` on tidy lines, with the exception summary if any."""
    try:
        message = record.getMessage()
    except Exception:  # noqa: BLE001
        message = str(record.msg)
    lines = [line.strip() for line in message.splitlines()]
    message = "\n".join(line for line in lines if line)
    exc = record.exc_info[1] if record.exc_info else None
    if exc is not None:
        summary = f"{type(exc).__name__}: {exc}"
        message = (
            summary if message.startswith("<class") or not message else f"{message} ({summary})"
        )
    return message


class ProtocolLogHandler(logging.Handler):
    """Forwards WARNING+ log records as protocol ``log`` events."""

    def __init__(self, emit: Callable[[Mapping[str, Any]], Any]) -> None:
        super().__init__(logging.WARNING)
        self._emit_event = emit

    def emit(self, record: logging.LogRecord) -> None:
        try:
            message = format_record(record)
            if not message or message.startswith(IGNORED_LOG_PREFIXES):
                return
            self._emit_event(log_event(record.levelname.lower(), message))
        except Exception:  # noqa: BLE001 - logging must never break the agent
            pass


def _prewarm_codecarbon() -> None:
    """Import CodeCarbon in the background so the first ``start`` is faster."""
    try:
        import codecarbon  # noqa: F401
        import codecarbon.core.units  # noqa: F401
        import codecarbon.output_methods.file  # noqa: F401
    except Exception:  # noqa: BLE001 - the real import error surfaces at start
        pass


class Runner:
    """Serves the protocol until ``quit``, EOF on stdin or a termination signal."""

    def __init__(
        self,
        stdin: LineSource,
        writer: Writer,
        session_factory: Callable[[], TrackerSession] | None = None,
        *,
        default_log_level: str = DEFAULT_LOG_LEVEL,
        install_signals: bool = False,
        prewarm: bool = False,
        stop_timeout_secs: float = 30.0,
        signal_grace_secs: float = 10.0,
        eof_grace_secs: float = 20.0,
    ) -> None:
        self._stdin = stdin
        self._writer = writer
        self._session = (
            session_factory()
            if session_factory is not None
            else TrackerSession(default_log_level=default_log_level)
        )
        self._install_signals = install_signals
        self._prewarm = prewarm
        self._stop_timeout = stop_timeout_secs
        self._signal_grace = signal_grace_secs
        self._eof_grace = eof_grace_secs
        self._session_lock = threading.Lock()
        self._lines: queue.Queue[str | None] = queue.Queue()
        self._terminate = threading.Event()
        self._sampler: threading.Thread | None = None
        self._sampler_stop: threading.Event | None = None
        self._last_measurement: dict[str, Any] | None = None
        self._reader: threading.Thread | None = None
        self._log_handler = ProtocolLogHandler(self._emit)
        self._previous_signals: dict[int, Any] = {}

    # ----------------------------------------------------------------- public

    @property
    def running(self) -> bool:
        return self._session.running

    @property
    def reader_alive(self) -> bool:
        """Whether the stdin reader thread is still blocked on stdin."""
        return self._reader is not None and self._reader.is_alive()

    def request_termination(self) -> None:
        """Ask the loop to stop the tracker and return (signal-safe)."""
        self._terminate.set()

    def run(self) -> int:
        """Serve until quit / EOF / signal; return the process exit code (0)."""
        self._setup()
        try:
            self._emit(ready_event())
            if self._prewarm:
                threading.Thread(target=_prewarm_codecarbon, daemon=True).start()
            self._reader = threading.Thread(target=self._read_stdin, name="stdin", daemon=True)
            self._reader.start()
            self._serve()
        finally:
            try:
                self._shutdown()
            finally:
                self._teardown()
        return 0

    # ----------------------------------------------------------------- setup

    def _setup(self) -> None:
        agent_logger = logging.getLogger("codecarbon_agent")
        if agent_logger.level == logging.NOTSET:
            agent_logger.setLevel(logging.WARNING)
        for name in FORWARDED_LOGGERS:
            logging.getLogger(name).addHandler(self._log_handler)
        if self._install_signals:
            self._set_signal_handlers()

    def _teardown(self) -> None:
        for name in FORWARDED_LOGGERS:
            logging.getLogger(name).removeHandler(self._log_handler)
        for signum, previous in self._previous_signals.items():
            with contextlib.suppress(ValueError, OSError):
                signal.signal(signum, previous)
        self._previous_signals.clear()

    def _set_signal_handlers(self) -> None:
        names = ("SIGTERM", "SIGINT", "SIGHUP", "SIGBREAK")
        for name in names:
            signum = getattr(signal, name, None)
            if signum is None:
                continue
            try:
                previous = signal.signal(signum, self._on_signal)
            except (ValueError, OSError):  # not the main thread / unsupported
                continue
            self._previous_signals.setdefault(signum, previous)

    def _on_signal(self, signum: int, frame: Any) -> None:
        if self._terminate.is_set():
            return
        self._terminate.set()
        # If the main thread is stuck (e.g. inside CodeCarbon's start), do not
        # outlive the request forever.
        watchdog = threading.Timer(self._signal_grace, self._force_exit)
        watchdog.daemon = True
        watchdog.start()

    def _force_exit(self) -> None:
        print(
            "codecarbon-agent: did not stop in time after a signal or EOF, exiting",
            file=sys.stderr,
        )
        with contextlib.suppress(Exception):
            sys.stderr.flush()
        os._exit(1)

    # ----------------------------------------------------------------- I/O

    def _emit(self, event: Mapping[str, Any]) -> None:
        self._writer.write(event)

    def _error(self, code: str, message: str) -> None:
        self._emit(error_event(code, message))

    def _read_stdin(self) -> None:
        try:
            while True:
                line = self._stdin.readline()
                if not line:
                    break
                self._lines.put(line)
        except Exception as exc:  # noqa: BLE001
            print(f"codecarbon-agent: stdin read failed: {exc!r}", file=sys.stderr)
        finally:
            self._lines.put(None)
            # EOF means the app is gone (quit or crashed). The main loop
            # normally notices within 0.1 s, but if it is stuck inside
            # CodeCarbon (e.g. a hanging ``start``) we must not become an
            # orphan at 100 % CPU: force the exit after a grace period.
            if self._eof_grace > 0:
                watchdog = threading.Timer(self._eof_grace, self._force_exit)
                watchdog.daemon = True
                watchdog.start()

    def _serve(self) -> None:
        while not self._terminate.is_set() and not self._writer.broken:
            try:
                line = self._lines.get(timeout=0.1)
            except queue.Empty:
                continue
            if line is None:  # EOF
                return
            if not self._handle_line(line):  # quit
                return

    # ----------------------------------------------------------------- commands

    def _handle_line(self, line: str) -> bool:
        """Handle one stdin line; return ``False`` when the agent must exit."""
        line = line.strip()
        if not line:
            return True
        try:
            return self._dispatch(line)
        except Exception as exc:  # noqa: BLE001
            traceback.print_exc(file=sys.stderr)
            self._error(INTERNAL, f"{type(exc).__name__}: {exc}")
            return True

    def _dispatch(self, line: str) -> bool:
        try:
            message = json.loads(line)
        except ValueError as exc:
            self._error(BAD_COMMAND, f"invalid JSON: {exc}")
            return True
        if not isinstance(message, dict):
            self._error(BAD_COMMAND, "a command must be a JSON object")
            return True
        cmd = message.get("cmd")
        handlers: dict[str, Callable[[dict[str, Any]], bool]] = {
            "start": self._cmd_start,
            "stop": self._cmd_stop,
            "status": self._cmd_status,
            "quit": self._cmd_quit,
        }
        handler = handlers.get(cmd) if isinstance(cmd, str) else None
        if handler is None:
            what = f"unknown command {cmd!r}" if cmd is not None else "missing 'cmd'"
            self._error(BAD_COMMAND, what)
            return True
        return handler(message)

    def _cmd_start(self, message: dict[str, Any]) -> bool:
        with self._session_lock:
            if self._session.running:
                self._error(ALREADY_RUNNING, "a measurement is already running")
                return True
            try:
                payload = self._session.start(message.get("config"))
            except StartError as exc:
                self._error(exc.code, exc.message)
                return True
            except Exception as exc:  # noqa: BLE001
                traceback.print_exc(file=sys.stderr)
                self._error(TRACKER_INIT_FAILED, f"{type(exc).__name__}: {exc}")
                return True
            if self._install_signals:  # a library may have replaced our handlers
                self._set_signal_handlers()
            self._last_measurement = None
            self._emit(started_event(payload))
            self._start_sampler(self._session.interval_secs)
        return True

    def _cmd_stop(self, message: dict[str, Any]) -> bool:
        if not self._session.running:
            self._error(NOT_RUNNING, "no measurement is running")
            return True
        self._stop_running()
        return True

    def _cmd_status(self, message: dict[str, Any]) -> bool:
        state = "running" if self._session.running else "idle"
        self._emit(status_event(state, self._last_measurement))
        return True

    def _cmd_quit(self, message: dict[str, Any]) -> bool:
        return False

    # ----------------------------------------------------------------- run lifecycle

    def _shutdown(self) -> None:
        """Stop a running tracker (emits ``stopped``); used by quit / EOF / signals."""
        try:
            if self._session.running:
                self._stop_running()
        except Exception as exc:  # noqa: BLE001
            traceback.print_exc(file=sys.stderr)
            self._error(INTERNAL, f"{type(exc).__name__}: {exc}")

    def _stop_running(self) -> None:
        stop_event, sampler = self._sampler_stop, self._sampler
        if stop_event is not None:
            stop_event.set()
        if sampler is not None and sampler is not threading.current_thread():
            sampler.join(self._stop_timeout)
        self._sampler, self._sampler_stop = None, None
        with self._session_lock:
            if not self._session.running:
                return
            try:
                summary = self._session.stop()
            except Exception as exc:  # noqa: BLE001
                traceback.print_exc(file=sys.stderr)
                self._error(INTERNAL, f"stop failed: {type(exc).__name__}: {exc}")
                return
            self._emit(stopped_event(summary))

    def _start_sampler(self, interval: float) -> None:
        stop_event = threading.Event()
        sampler = threading.Thread(
            target=self._sample_loop, args=(stop_event, interval), name="sampler", daemon=True
        )
        self._sampler, self._sampler_stop = sampler, stop_event
        sampler.start()

    def _sample_loop(self, stop_event: threading.Event, interval: float) -> None:
        """Emit a ``measurement`` every ``interval`` seconds until ``stop_event``."""
        next_tick = time.monotonic() + interval
        failing = False
        while not stop_event.wait(max(0.0, next_tick - time.monotonic())):
            try:
                failing = self._sample_once(stop_event, failing)
            except Exception as exc:  # noqa: BLE001 - keep sampling no matter what
                traceback.print_exc(file=sys.stderr)
                self._error(INTERNAL, f"sampler: {type(exc).__name__}: {exc}")
            now = time.monotonic()
            next_tick += interval
            while next_tick <= now:  # a slow flush: skip the missed ticks
                next_tick += interval

    def _sample_once(self, stop_event: threading.Event, failing: bool) -> bool:
        """Take one sample; return whether sampling is currently failing."""
        with self._session_lock:
            if stop_event.is_set() or not self._session.running:
                return failing
            try:
                payload = self._session.sample()
            except Exception as exc:  # noqa: BLE001
                if not failing:  # report once per streak of failures
                    self._error(INTERNAL, f"measurement failed: {exc}")
                return True
            event = measurement_event(payload)
            self._last_measurement = event
            self._emit(event)
            return False
