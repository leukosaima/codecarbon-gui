#!/usr/bin/env python3
"""A fake ``codecarbon-agent`` for UI development.

It speaks the JSON-lines protocol described in docs/PROTOCOL.md but does not
measure anything: it invents plausible, slowly drifting power figures. Use it
when you work on the Tauri app and do not want to build the real PyInstaller
sidecar (see scripts/use-fake-agent.sh).

Only the standard library is used, so any Python 3.8+ works.
"""

from __future__ import annotations

import json
import math
import os
import platform
import random
import sys
import threading
import time
import uuid
from datetime import datetime

PROTOCOL_VERSION = 1
AGENT_VERSION = "0.0.0-fake"
CARBON_INTENSITY = 56.0  # g/kWh, roughly France

_stdout_lock = threading.Lock()


def now_iso() -> str:
    """Local time with UTC offset, e.g. 2026-11-02T20:50:26+01:00."""
    return datetime.now().astimezone().isoformat(timespec="seconds")


def emit(event: str, **fields) -> None:
    """Write one event as a single JSON line on stdout."""
    payload = {"event": event, "ts": now_iso(), **fields}
    with _stdout_lock:
        sys.stdout.write(json.dumps(payload, separators=(",", ":")) + "\n")
        sys.stdout.flush()


def log(msg: str) -> None:
    """Diagnostics go to stderr, never stdout."""
    sys.stderr.write(f"[fake-agent] {msg}\n")
    sys.stderr.flush()


class FakeTracker:
    """Produces synthetic measurements on a background thread."""

    def __init__(self, config: dict) -> None:
        self.config = config
        self.interval = max(0.2, float(config.get("interval_secs", 3) or 3))
        self.run_id = str(uuid.uuid4())
        self.started_at = now_iso()
        self.t0 = time.monotonic()
        self.energy = {"cpu": 0.0, "gpu": 0.0, "ram": 0.0}
        self.intensity = float(
            config.get("force_carbon_intensity_g_co2e_kwh") or CARBON_INTENSITY
        )
        self.pue = float(config.get("pue") or 1.0)
        self.last_measurement: dict | None = None
        self._stop = threading.Event()
        self._last_t = self.t0
        self._phase = random.random() * 10
        self._thread = threading.Thread(target=self._loop, daemon=True)

    # -- public API -------------------------------------------------------
    def start(self) -> None:
        country = self.config.get("country_iso_code") or "FRA"
        emit(
            "started",
            run_id=self.run_id,
            started_at=self.started_at,
            hardware={
                "cpu_model": "Fake CPU 9000 @ 3.0GHz",
                "cpu_count": 8,
                "cpu_physical_count": 4,
                "cpu_method": "cpu_load",
                "cpu_tdp_w": 28,
                "cpu_tdp_is_generic": False,
                "gpu_model": None,
                "gpu_count": 0,
                "gpu_method": None,
                "ram_total_gb": 16.0,
                "ram_method": "estimate",
                "descriptions": ["RAM()", "CPU(Cpu Load)"],
            },
            location={
                "country_name": "France" if country == "FRA" else country,
                "country_iso_code": country,
                "region": self.config.get("region"),
                "carbon_intensity_g_per_kwh": self.intensity,
            },
            config=self.config,
        )
        emit(
            "log",
            level="warning",
            message="Fake agent: these numbers are synthetic, nothing is measured.",
        )
        self._thread.start()

    def stop(self) -> dict:
        self._stop.set()
        self._thread.join(timeout=2)
        self._sample()  # account for the time since the last tick
        duration = time.monotonic() - self.t0
        total_kwh = sum(self.energy.values())
        emissions = self._emissions(total_kwh)
        avg = {k: (v * 3.6e6 / duration if duration > 0 else 0.0) for k, v in self.energy.items()}
        avg["total"] = sum(avg.values())
        return {
            "run_id": self.run_id,
            "started_at": self.started_at,
            "stopped_at": now_iso(),
            "duration_s": round(duration, 3),
            "energy_kwh": {**self.energy, "total": total_kwh},
            "emissions_kg": emissions,
            "emissions_rate_kg_s": emissions / duration if duration > 0 else 0.0,
            "avg_power_w": avg,
            "carbon_intensity_g_per_kwh": self.intensity,
        }

    # -- internals ---------------------------------------------------------
    def _emissions(self, kwh: float) -> float:
        return kwh * self.pue * self.intensity / 1000.0

    def _power(self, t: float) -> dict:
        """Drifting CPU power, constant-ish RAM, no GPU."""
        cpu = 9.0 + 6.0 * math.sin((t + self._phase) / 20.0) + random.uniform(-1.5, 1.5)
        cpu = max(1.0, cpu)
        ram = 3.0 + random.uniform(-0.1, 0.1)
        return {"cpu": cpu, "gpu": 0.0, "ram": ram}

    def _sample(self) -> dict:
        now = time.monotonic()
        dt = now - self._last_t
        self._last_t = now
        power = self._power(now - self.t0)
        for k, w in power.items():
            self.energy[k] += w * dt / 3.6e6  # W·s -> kWh
        return power

    def _loop(self) -> None:
        while not self._stop.wait(self.interval):
            power = self._sample()
            duration = time.monotonic() - self.t0
            total_kwh = sum(self.energy.values())
            emissions = self._emissions(total_kwh)
            total_w = sum(power.values())
            m = {
                "duration_s": round(duration, 3),
                "power_w": {**{k: round(v, 3) for k, v in power.items()}, "total": round(total_w, 3)},
                "energy_kwh": {**self.energy, "total": total_kwh},
                "emissions_kg": emissions,
                "emissions_rate_kg_s": emissions / duration if duration > 0 else 0.0,
                "cpu_utilization_percent": round(min(100.0, power["cpu"] * 4), 1),
                "ram_used_gb": round(7.0 + random.uniform(-0.3, 0.3), 2),
                "carbon_intensity_g_per_kwh": self.intensity,
            }
            self.last_measurement = m
            emit("measurement", **m)


def main() -> int:
    if "--version" in sys.argv[1:]:
        print(f"codecarbon-agent {AGENT_VERSION} (codecarbon fake)")
        return 0

    log(f"starting, pid {os.getpid()}")
    emit(
        "ready",
        protocol_version=PROTOCOL_VERSION,
        agent_version=AGENT_VERSION,
        codecarbon_version="fake",
        python_version=platform.python_version(),
        pid=os.getpid(),
        platform=platform.platform(),
    )

    tracker: FakeTracker | None = None
    for raw in sys.stdin:
        line = raw.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
            cmd = msg["cmd"]
        except (ValueError, KeyError, TypeError):
            emit("error", code="bad_command", message=f"cannot parse: {line[:200]}")
            continue
        log(f"command: {cmd}")

        if cmd == "start":
            if tracker is not None:
                emit("error", code="already_running", message="A measurement is already running.")
                continue
            config = msg.get("config") or {}
            tracker = FakeTracker(config if isinstance(config, dict) else {})
            tracker.start()
        elif cmd == "stop":
            if tracker is None:
                emit("error", code="not_running", message="No measurement is running.")
                continue
            summary = tracker.stop()
            tracker = None
            emit("stopped", summary=summary)
        elif cmd == "status":
            emit(
                "status",
                state="running" if tracker else "idle",
                last_measurement=tracker.last_measurement if tracker else None,
            )
        elif cmd == "quit":
            break
        else:
            emit("error", code="bad_command", message=f"unknown command: {cmd}")

    if tracker is not None:
        emit("stopped", summary=tracker.stop())
    log("bye")
    return 0


if __name__ == "__main__":
    sys.exit(main())
