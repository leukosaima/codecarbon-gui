"""Command-line entry point of ``codecarbon-agent``.

``codecarbon-agent``              protocol mode (JSON lines on stdin / stdout)
``codecarbon-agent --version``    print the versions
``codecarbon-agent --selftest``   short real measurement, exit 0 on success
``codecarbon-agent --credentials <file>``   enables the dashboard login (protocol v2)
"""

from __future__ import annotations

import argparse
import contextlib
import os
import queue
import sys
import threading
import time
import traceback
from collections.abc import Mapping, Sequence
from typing import IO, Any

from . import __version__
from .compat import patch_cpuinfo
from .protocol import Writer, codecarbon_version
from .runner import Runner
from .tracker import LOG_LEVELS

EXIT_OK, EXIT_CRASH, EXIT_USAGE = 0, 1, 2
SELFTEST_START_TIMEOUT_SECS = 120.0


def version_line() -> str:
    return f"codecarbon-agent {__version__} (codecarbon {codecarbon_version() or 'not installed'})"


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="codecarbon-agent",
        description="CodeCarbon sidecar: JSON-lines protocol on stdin/stdout "
        "(see docs/PROTOCOL.md).",
    )
    parser.add_argument("--version", action="store_true", help="print the version and exit")
    parser.add_argument(
        "--selftest", action="store_true", help="run a short real measurement and exit"
    )
    parser.add_argument(
        "--seconds",
        type=_positive_float,
        default=None,
        help="duration of --selftest in seconds (default 3)",
    )
    parser.add_argument(
        "--credentials",
        metavar="PATH",
        default=None,
        help="credentials file of the CodeCarbon dashboard login (created with mode 0600); "
        "without it the auth_* / api_* commands reply error(auth_unavailable)",
    )
    parser.add_argument(
        "--log-level",
        choices=sorted(LOG_LEVELS),
        default="warning",
        help="default CodeCarbon log level (stderr); config.log_level overrides it",
    )
    return parser


def _positive_float(value: str) -> float:
    try:
        number = float(value)
    except ValueError:
        raise argparse.ArgumentTypeError(f"not a number: {value!r}") from None
    if not number > 0 or number == float("inf"):
        raise argparse.ArgumentTypeError("must be a positive number")
    return number


def take_over_stdout() -> IO[str]:
    """Reserve the real stdout for protocol events.

    Returns a private stream on the original stdout, then points file
    descriptor 1 *and* ``sys.stdout`` at stderr, so stray prints, C-level
    output and child processes can never corrupt the protocol stream.
    """
    stream: IO[str]
    try:
        sys.stdout.flush()
        fd = os.dup(1)
        os.dup2(2, 1)
        stream = os.fdopen(fd, "w", encoding="utf-8", newline="\n")
    except (OSError, ValueError, AttributeError):
        stream = sys.stdout
    sys.stdout = sys.stderr if sys.stderr is not None else open(os.devnull, "w")  # noqa: SIM115
    return stream


def _utf8_stdin() -> None:
    with contextlib.suppress(Exception):
        sys.stdin.reconfigure(encoding="utf-8", errors="replace")  # type: ignore[union-attr]


# --------------------------------------------------------------------------- selftest


class _QueueInput:
    """A ``readline()`` source fed by the selftest driver."""

    def __init__(self) -> None:
        self._queue: queue.Queue[str] = queue.Queue()

    def push(self, line: str) -> None:
        self._queue.put(line + "\n")

    def close(self) -> None:
        self._queue.put("")

    def readline(self) -> str:
        return self._queue.get()


class RecordingWriter(Writer):
    """A :class:`Writer` that also keeps the events, for the selftest verdict."""

    def __init__(self, stream: IO[str]) -> None:
        super().__init__(stream)
        self.events: list[dict[str, Any]] = []
        self._cond = threading.Condition()

    def write(self, event: Mapping[str, Any]) -> bool:
        with self._cond:
            self.events.append(dict(event))
            self._cond.notify_all()
        return super().write(event)

    def wait_for(self, names: set[str], timeout: float) -> dict[str, Any] | None:
        """Wait for a new event whose name is in ``names``."""
        deadline = time.monotonic() + timeout
        with self._cond:
            seen = len(self.events)
            while True:
                for event in self.events[seen:]:
                    if event.get("event") in names:
                        return event
                seen = len(self.events)
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return None
                self._cond.wait(remaining)


def _selftest_driver(
    stdin: _QueueInput, writer: RecordingWriter, seconds: float, log_level: str
) -> None:
    import json

    config = {
        "interval_secs": max(0.5, min(1.0, seconds / 3)),
        "country_iso_code": "FRA",
        "save_to_file": False,
        "log_level": log_level,
    }
    try:
        stdin.push(json.dumps({"cmd": "start", "config": config}))
        reply = writer.wait_for({"started", "error"}, SELFTEST_START_TIMEOUT_SECS)
        if reply is not None and reply.get("event") == "started":
            time.sleep(seconds)
            stdin.push(json.dumps({"cmd": "stop"}))
            writer.wait_for({"stopped"}, 60.0)
        stdin.push(json.dumps({"cmd": "quit"}))
    finally:
        stdin.close()


def selftest_verdict(events: Sequence[Mapping[str, Any]]) -> tuple[bool, str]:
    names = [e.get("event") for e in events]
    counts = {n: names.count(n) for n in ("ready", "started", "measurement", "stopped", "error")}
    ok = counts["started"] >= 1 and counts["measurement"] >= 1 and counts["stopped"] >= 1
    detail = ", ".join(f"{n}={c}" for n, c in counts.items())
    return ok, detail


def run_selftest(seconds: float, out: IO[str], log_level: str) -> int:
    """Run ``start`` / wait / ``stop`` / ``quit`` through the real runner and tracker."""
    stdin = _QueueInput()
    writer = RecordingWriter(out)
    runner = Runner(stdin, writer, default_log_level=log_level, install_signals=True)
    driver = threading.Thread(
        target=_selftest_driver, args=(stdin, writer, seconds, log_level), daemon=True
    )
    driver.start()
    runner.run()
    ok, detail = selftest_verdict(writer.events)
    print(f"selftest: {'OK' if ok else 'FAILED'} ({detail})", file=sys.stderr)
    return EXIT_OK if ok else EXIT_CRASH


# --------------------------------------------------------------------------- main


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)  # exits with code 2 on bad usage
    if args.seconds is not None and not args.selftest:
        parser.error("--seconds requires --selftest")
    if args.version:
        print(version_line())
        return EXIT_OK

    # CodeCarbon applies this level when it is imported (before any tracker exists).
    os.environ.setdefault("CODECARBON_LOG_LEVEL", args.log_level)
    patch_cpuinfo()
    protocol_out = take_over_stdout()
    _utf8_stdin()
    runner: Runner | None = None
    try:
        if args.selftest:
            code = run_selftest(args.seconds or 3.0, protocol_out, args.log_level)
        else:
            runner = Runner(
                sys.stdin,
                Writer(protocol_out),
                default_log_level=args.log_level,
                install_signals=True,
                prewarm=True,
                credentials_path=args.credentials,
            )
            code = runner.run()
    except Exception:  # noqa: BLE001
        traceback.print_exc(file=sys.stderr)
        code = EXIT_CRASH
    for stream in (protocol_out, sys.stderr):
        with contextlib.suppress(Exception):
            stream.flush()
    if runner is not None:
        # Daemon threads may still be blocked on stdin (after ``quit`` or a
        # signal) or importing CodeCarbon: skip interpreter finalisation, which
        # could deadlock on stdin's lock. The tracker is already stopped.
        os._exit(code)
    return code


if __name__ == "__main__":
    sys.exit(main())
