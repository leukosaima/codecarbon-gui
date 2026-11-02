"""Shared fakes: a fake CodeCarbon tracker and a harness driving the Runner in memory."""

from __future__ import annotations

import io
import json
import queue
import threading
import time
from collections.abc import Callable
from types import SimpleNamespace
from typing import Any

import pytest

from codecarbon_agent.protocol import Writer
from codecarbon_agent.runner import Runner
from codecarbon_agent.tracker import TrackerSession


class FakePower:
    def __init__(self, watts: float) -> None:
        self.W = watts


# Class names matter: the agent recognises hardware by class name (like CodeCarbon's).
class CPU:
    _mode = "cpu_load"
    _tdp = 28
    _is_generic_tdp = False

    def description(self) -> str:
        return "CPU(Cpu Load)"


class RAM:
    _force_ram_power = None

    def description(self) -> str:
        return "RAM()"


def make_data(duration: float, **overrides: Any) -> SimpleNamespace:
    """An EmissionsData look-alike: 10 W CPU + 5 W RAM, 50 g/kWh."""
    cpu_kwh = 10 * duration / 3.6e6
    ram_kwh = 5 * duration / 3.6e6
    total = cpu_kwh + ram_kwh
    values = dict(
        run_id="run-123",
        duration=duration,
        emissions=total * 0.05,
        emissions_rate=(total * 0.05 / duration) if duration else 0.0,
        cpu_power=10.0,
        gpu_power=0.0,
        ram_power=5.0,
        cpu_energy=cpu_kwh,
        gpu_energy=0.0,
        ram_energy=ram_kwh,
        energy_consumed=total,
        cpu_utilization_percent=12.5,
        ram_used_gb=7.4,
        country_name="France",
        country_iso_code="FRA",
        region=None,
    )
    values.update(overrides)
    return SimpleNamespace(**values)


class FakeTracker:
    """Mimics the parts of a CodeCarbon tracker the agent uses."""

    def __init__(
        self,
        offline: bool,
        kwargs: dict[str, Any],
        *,
        flush_delay: float = 0.0,
        fail_flushes: int = 0,
        data_overrides: dict[str, Any] | None = None,
    ) -> None:
        self.offline = offline
        self.kwargs = kwargs
        self.handlers = kwargs["output_handlers"]
        self.flush_delay = flush_delay
        self.fail_flushes = fail_flushes
        self.data_overrides = data_overrides or {}
        self.run_id = "run-123"
        self._start_time: float | None = None
        self._conf = {
            "cpu_model": "Fake CPU 9000",
            "cpu_count": 8,
            "cpu_physical_count": 4,
            "gpu_model": "",
            "gpu_count": 0,
            "ram_total_size": 31.1234,
            "hardware": ["RAM()", "CPU(Cpu Load)"],
        }
        self._hardware = [RAM(), CPU()]
        self._cpu_power = FakePower(7.5)
        self._gpu_power = FakePower(0.0)
        self._ram_power = FakePower(5.0)
        self.flushes = 0
        self.stopped = False

    def start(self) -> None:
        self._start_time = time.monotonic()

    def _emit(self) -> None:
        data = make_data(time.monotonic() - self._start_time, **self.data_overrides)
        for handler in self.handlers:
            handler.out(data, data)

    def flush(self) -> float | None:
        self.flushes += 1
        if self.flush_delay:
            time.sleep(self.flush_delay)
        if self.fail_flushes > 0:
            self.fail_flushes -= 1
            return None  # like CodeCarbon's @suppress: silently nothing
        self._emit()
        return 0.0

    def stop(self) -> float | None:
        self._emit()
        for handler in self.handlers:
            handler.exit()
        self._start_time = None
        self.stopped = True
        return 0.0


class FakeCsv:
    instances: list[FakeCsv] = []

    def __init__(self, output_dir: str) -> None:
        self.path = f"{output_dir}/emissions.csv"
        self.rows: list[Any] = []
        self._latest = None
        FakeCsv.instances.append(self)

    def out(self, total: Any, delta: Any) -> None:
        self._latest = total

    def exit(self) -> None:
        self.write()

    def discard(self) -> None:
        self._latest = None

    def write(self) -> None:
        if self._latest is not None and not self.rows:
            self.rows.append(self._latest)


def fake_session(**tracker_options: Any) -> tuple[TrackerSession, list[FakeTracker]]:
    created: list[FakeTracker] = []

    def factory(offline: bool, kwargs: dict[str, Any]) -> FakeTracker:
        tracker = FakeTracker(offline, kwargs, **tracker_options)
        created.append(tracker)
        return tracker

    session = TrackerSession(
        tracker_factory=factory, csv_factory=FakeCsv, country_codes=lambda: {"FRA", "USA"}
    )
    return session, created


class PushInput:
    """A stdin replacement fed line by line from the test."""

    def __init__(self) -> None:
        self._queue: queue.Queue[str] = queue.Queue()

    def send(self, line: str) -> None:
        self._queue.put(line + "\n")

    def close(self) -> None:
        self._queue.put("")

    def readline(self) -> str:
        return self._queue.get()


class Harness:
    """Runs a Runner in a thread over in-memory stdin / stdout."""

    def __init__(self, session_factory: Callable[[], TrackerSession]) -> None:
        self.stdin = PushInput()
        self.stdout = io.StringIO()
        self.writer = Writer(self.stdout)
        self.runner = Runner(self.stdin, self.writer, session_factory)
        self.exit_code: int | None = None
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def _run(self) -> None:
        self.exit_code = self.runner.run()

    def send(self, obj: Any) -> None:
        self.stdin.send(obj if isinstance(obj, str) else json.dumps(obj))

    def events(self) -> list[dict[str, Any]]:
        return [json.loads(line) for line in self.stdout.getvalue().splitlines()]

    def names(self) -> list[str]:
        return [e["event"] for e in self.events()]

    def wait_for(
        self, predicate: Callable[[dict[str, Any]], bool], timeout: float = 5.0, after: int = 0
    ) -> dict[str, Any]:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            for event in self.events()[after:]:
                if predicate(event):
                    return event
            time.sleep(0.01)
        raise AssertionError(f"event not seen; got {self.names()}")

    def wait_event(self, name: str, timeout: float = 5.0, after: int = 0) -> dict[str, Any]:
        return self.wait_for(lambda e: e["event"] == name, timeout, after)

    def request(self, obj: Any, name: str, timeout: float = 5.0) -> dict[str, Any]:
        """Send a command and wait for the next event called ``name``."""
        seen = len(self.events())
        self.send(obj)
        return self.wait_event(name, timeout, after=seen)

    def join(self, timeout: float = 10.0) -> int | None:
        self._thread.join(timeout)
        assert not self._thread.is_alive(), "runner did not exit"
        return self.exit_code


@pytest.fixture
def harness_factory():
    harnesses: list[Harness] = []

    def make(**tracker_options: Any) -> tuple[Harness, list[FakeTracker]]:
        session, created = fake_session(**tracker_options)
        harness = Harness(lambda: session)
        harnesses.append(harness)
        harness.wait_event("ready")
        return harness, created

    yield make
    for harness in harnesses:
        harness.stdin.close()
        harness.join()
