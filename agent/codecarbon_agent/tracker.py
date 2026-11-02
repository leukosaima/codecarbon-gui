"""A single measurement session on top of a CodeCarbon tracker.

:class:`TrackerSession` turns protocol ``config`` objects into a CodeCarbon
``EmissionsTracker`` / ``OfflineEmissionsTracker`` and its numbers into the
``started`` / ``measurement`` / ``stopped`` payloads of ``docs/PROTOCOL.md``.

CodeCarbon internals (``tracker._cpu_power``, ``tracker._hardware``, ...) are
read defensively: if they move in a future release the corresponding fields
become ``null``, nothing crashes. The tracker class is injected through a
factory so tests can run without touching real hardware.
"""

from __future__ import annotations

import contextlib
import logging
import math
import os
import threading
from collections.abc import Callable, Iterable, Mapping
from typing import Any

from .protocol import ALREADY_RUNNING, BAD_COMMAND, TRACKER_INIT_FAILED, now_iso

logger = logging.getLogger("codecarbon_agent")

DEFAULT_INTERVAL_SECS = 3.0
MIN_INTERVAL_SECS = 0.5
DEFAULT_PROJECT_NAME = "codecarbon-gui"
DEFAULT_API_ENDPOINT = "https://api.codecarbon.io"
DEFAULT_LOG_LEVEL = "warning"
CSV_FILE_NAME = "emissions.csv"
LOG_LEVELS = frozenset({"debug", "info", "warning", "error", "critical"})
TRACKING_MODES = frozenset({"machine", "process"})

# (offline, kwargs) -> tracker object with start() / flush() / stop()
TrackerFactory = Callable[[bool, dict[str, Any]], Any]
# output_dir -> output handler that writes the CSV row of the run
CsvFactory = Callable[[str], Any]


class StartError(Exception):
    """A ``start`` that cannot proceed; ``code`` is a protocol error code."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


class SampleError(Exception):
    """A measurement could not be produced (CodeCarbon swallowed an error)."""


# --------------------------------------------------------------------------- numbers


def num(value: Any) -> float | None:
    """``value`` as a finite float, else ``None`` (never NaN / Infinity)."""
    if value is None or isinstance(value, bool):
        return None
    try:
        result = float(value)
    except (TypeError, ValueError):
        return None
    return result if math.isfinite(result) else None


def integer(value: Any) -> int | None:
    number = num(value)
    return int(number) if number is not None else None


def total(*values: float | None) -> float | None:
    """Sum of the non-null values, ``None`` if all are null."""
    present = [v for v in values if v is not None]
    return sum(present) if present else None


def safe_getattr(obj: Any, *path: str) -> Any:
    """``obj.a.b.c`` for ``path=("a", "b", "c")``; ``None`` if anything fails."""
    try:
        for name in path:
            if obj is None:
                return None
            obj = getattr(obj, name)
        return obj
    except Exception:  # noqa: BLE001 - properties of foreign objects may raise anything
        return None


def text(value: Any) -> str | None:
    """Non-empty string or ``None``."""
    if value is None:
        return None
    result = str(value).strip()
    return result or None


# --------------------------------------------------------------------------- config


def _opt_str(raw: Mapping[str, Any], key: str) -> str | None:
    value = raw.get(key)
    if value is None:
        return None
    if not isinstance(value, str):
        raise StartError(BAD_COMMAND, f"config.{key} must be a string")
    return value.strip() or None


def _bool(raw: Mapping[str, Any], key: str, default: bool) -> bool:
    value = raw.get(key)
    if value is None:
        return default
    if not isinstance(value, bool):
        raise StartError(BAD_COMMAND, f"config.{key} must be a boolean")
    return value


def _opt_number(raw: Mapping[str, Any], key: str) -> float | None:
    value = raw.get(key)
    if value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise StartError(BAD_COMMAND, f"config.{key} must be a number")
    if not math.isfinite(value):
        raise StartError(BAD_COMMAND, f"config.{key} must be finite")
    return float(value)


def normalize_config(
    raw: Mapping[str, Any] | None, default_log_level: str = DEFAULT_LOG_LEVEL
) -> dict[str, Any]:
    """Validate a protocol ``config`` object and fill in the defaults.

    Unknown keys are ignored. Type errors raise :class:`StartError` with
    ``bad_command``.
    """
    if raw is None:
        raw = {}
    if not isinstance(raw, Mapping):
        raise StartError(BAD_COMMAND, "config must be a JSON object")

    interval = _opt_number(raw, "interval_secs")
    if interval is None:
        interval = DEFAULT_INTERVAL_SECS
    if interval <= 0:
        raise StartError(BAD_COMMAND, "config.interval_secs must be > 0")
    interval = max(interval, MIN_INTERVAL_SECS)

    country = _opt_str(raw, "country_iso_code")
    if country is not None:
        country = country.upper()
        if len(country) != 3 or not country.isalpha():
            raise StartError(BAD_COMMAND, "config.country_iso_code must be a 3-letter ISO code")

    tracking_mode = (_opt_str(raw, "tracking_mode") or "machine").lower()
    if tracking_mode not in TRACKING_MODES:
        raise StartError(BAD_COMMAND, "config.tracking_mode must be 'machine' or 'process'")

    log_level = (_opt_str(raw, "log_level") or default_log_level).lower()
    if log_level == "warn":
        log_level = "warning"
    if log_level not in LOG_LEVELS:
        raise StartError(BAD_COMMAND, f"config.log_level must be one of {sorted(LOG_LEVELS)}")

    intensity = _opt_number(raw, "force_carbon_intensity_g_co2e_kwh")
    if intensity is not None and intensity < 0:
        raise StartError(BAD_COMMAND, "config.force_carbon_intensity_g_co2e_kwh must be >= 0")
    pue = _opt_number(raw, "pue")
    if pue is not None and pue <= 0:
        raise StartError(BAD_COMMAND, "config.pue must be > 0")

    output_dir = _opt_str(raw, "output_dir") or os.getcwd()
    return {
        "interval_secs": interval,
        "country_iso_code": country,
        "region": _opt_str(raw, "region"),
        "tracking_mode": tracking_mode,
        "project_name": _opt_str(raw, "project_name") or DEFAULT_PROJECT_NAME,
        "save_to_file": _bool(raw, "save_to_file", True),
        "output_dir": os.path.abspath(os.path.expanduser(output_dir)),
        "save_to_api": _bool(raw, "save_to_api", False),
        "api_endpoint": _opt_str(raw, "api_endpoint") or DEFAULT_API_ENDPOINT,
        "api_key": _opt_str(raw, "api_key"),
        "experiment_id": _opt_str(raw, "experiment_id"),
        "force_carbon_intensity_g_co2e_kwh": intensity,
        "pue": pue,
        "log_level": log_level,
    }


def public_config(config: Mapping[str, Any]) -> dict[str, Any]:
    """The effective config echoed in ``started``, with the API key redacted."""
    result = dict(config)
    if result.get("api_key"):
        result["api_key"] = "***"
    return result


def known_country_codes() -> set[str] | None:
    """3-letter codes CodeCarbon has energy-mix data for; ``None`` if unknown."""
    try:
        from codecarbon.input import DataSource

        return {str(code).upper() for code in DataSource().get_global_energy_mix_data()}
    except Exception:  # noqa: BLE001
        return None


def tracker_kwargs(config: Mapping[str, Any], handlers: list[Any]) -> dict[str, Any]:
    """Keyword arguments for the CodeCarbon tracker constructor.

    ``save_to_file`` is always ``False`` for CodeCarbon itself: :class:`FinalCsvOutput`
    appends the run's row once, at stop, instead of once per ``flush()``.
    """
    kwargs: dict[str, Any] = {
        "measure_power_secs": config["interval_secs"],
        "save_to_file": False,
        "output_dir": config["output_dir"],
        "project_name": config["project_name"],
        "tracking_mode": config["tracking_mode"],
        "log_level": config["log_level"],
        "allow_multiple_runs": True,
        "output_handlers": handlers,
        "save_to_api": config["save_to_api"],
    }
    if config["save_to_api"]:
        kwargs["api_endpoint"] = config["api_endpoint"]
        kwargs["api_key"] = config["api_key"]
        kwargs["experiment_id"] = config["experiment_id"]
    if config["pue"] is not None:
        kwargs["pue"] = config["pue"]
    if config["force_carbon_intensity_g_co2e_kwh"] is not None:
        kwargs["force_carbon_intensity_g_co2e_kwh"] = config["force_carbon_intensity_g_co2e_kwh"]
    if config["country_iso_code"] is not None:
        kwargs["country_iso_code"] = config["country_iso_code"]
        if config["region"] is not None:
            kwargs["region"] = config["region"]
    return kwargs


def default_tracker_factory(offline: bool, kwargs: dict[str, Any]) -> Any:
    """Create the real CodeCarbon tracker (imported lazily: it is slow to import)."""
    if offline:
        from codecarbon import OfflineEmissionsTracker

        return OfflineEmissionsTracker(**kwargs)
    from codecarbon import EmissionsTracker

    return EmissionsTracker(**kwargs)


# --------------------------------------------------------------------------- output handlers
# CodeCarbon only duck-types its output handlers (out / live_out / task_out /
# exit), so these do not inherit from BaseOutput: no codecarbon import needed.


class CaptureOutput:
    """Keeps the latest ``EmissionsData`` that CodeCarbon produced."""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._latest: Any = None
        self._count = 0

    def out(self, total_data: Any, delta: Any) -> None:
        with self._lock:
            self._latest = total_data
            self._count += 1

    def live_out(self, total_data: Any, delta: Any) -> None:
        pass

    def task_out(self, data: Any, experiment_name: str) -> None:
        pass

    def exit(self) -> None:
        pass

    def snapshot(self) -> tuple[int, Any]:
        """``(number of out() calls so far, latest EmissionsData or None)``."""
        with self._lock:
            return self._count, self._latest


class FinalCsvOutput:
    """Appends exactly one row per run to ``emissions.csv``, when the run stops.

    CodeCarbon's own CSV output appends a row on every ``flush()``, which the
    agent calls every ``interval_secs``. This wrapper only remembers the
    latest totals and hands them to CodeCarbon's ``FileOutput`` on ``exit()``,
    so the CSV format stays exactly CodeCarbon's.
    """

    def __init__(self, output_dir: str, file_name: str = CSV_FILE_NAME) -> None:
        from codecarbon.output_methods.file import FileOutput

        self.path = os.path.join(output_dir, file_name)
        self._file = FileOutput(file_name, output_dir, "append")
        self._lock = threading.Lock()
        self._latest: Any = None
        self._written = False

    def out(self, total_data: Any, delta: Any) -> None:
        with self._lock:
            self._latest = total_data

    def live_out(self, total_data: Any, delta: Any) -> None:
        pass

    def task_out(self, data: Any, experiment_name: str) -> None:
        pass

    def exit(self) -> None:
        self.write()

    def discard(self) -> None:
        """Never write (used when a start fails half-way)."""
        with self._lock:
            self._written = True

    def write(self) -> None:
        """Write the row once; later calls are no-ops. Never raises."""
        with self._lock:
            if self._written or self._latest is None:
                return
            self._written = True
            latest = self._latest
        try:
            self._file.out(latest, None)
        except Exception as exc:  # noqa: BLE001
            logger.warning("Could not write the run to %s: %s", self.path, exc)


# --------------------------------------------------------------------------- extraction


def _class_names(obj: Any) -> set[str]:
    try:
        return {cls.__name__ for cls in type(obj).__mro__}
    except Exception:  # noqa: BLE001
        return set()


def _gpu_method(gpu: Any) -> str:
    devices = safe_getattr(gpu, "devices", "devices") or []
    try:
        names = " ".join(type(d).__name__.lower() for d in devices)
    except Exception:  # noqa: BLE001
        names = ""
    return "amdsmi" if "amd" in names else "nvml"


def _conf(tracker: Any) -> dict[str, Any]:
    conf = safe_getattr(tracker, "_conf")
    return dict(conf) if isinstance(conf, Mapping) else {}


def _hardware_list(tracker: Any) -> list[Any]:
    hardware = safe_getattr(tracker, "_hardware")
    try:
        return list(hardware or [])
    except TypeError:
        return []


def _descriptions(conf: Mapping[str, Any], hardware: Iterable[Any]) -> list[str]:
    described = conf.get("hardware")
    if isinstance(described, (list, tuple)):
        return [str(d) for d in described]
    result = []
    for item in hardware:
        try:
            result.append(str(item.description()))
        except Exception:  # noqa: BLE001
            result.append(type(item).__name__)
    return result


def extract_hardware(tracker: Any) -> dict[str, Any]:
    """The ``started.hardware`` object, from ``tracker._hardware`` and ``tracker._conf``."""
    conf = _conf(tracker)
    hardware = _hardware_list(tracker)
    info: dict[str, Any] = {
        "cpu_model": text(conf.get("cpu_model")),
        "cpu_count": integer(conf.get("cpu_count")),
        "cpu_physical_count": integer(conf.get("cpu_physical_count")),
        "cpu_method": None,
        "cpu_tdp_w": None,
        "cpu_tdp_is_generic": None,
        "gpu_model": text(conf.get("gpu_model")),
        "gpu_count": integer(conf.get("gpu_count")) or 0,
        "gpu_method": None,
        "ram_total_gb": None,
        "ram_method": None,
        "descriptions": _descriptions(conf, hardware),
    }
    ram_total = num(conf.get("ram_total_size"))
    info["ram_total_gb"] = round(ram_total, 2) if ram_total is not None else None

    for item in hardware:
        names = _class_names(item)
        part = str(safe_getattr(item, "chip_part") or "").upper()
        if "AppleSiliconChip" in names:
            key = "gpu_method" if part == "GPU" else "cpu_method"
            info[key] = "apple_powermetrics"
        elif "Raspberry" in names:
            key = "ram_method" if part == "RAM" else "cpu_method"
            info[key] = "raspberry_pmic"
        elif "CPU" in names:
            info["cpu_method"] = text(safe_getattr(item, "_mode"))
            tdp = num(safe_getattr(item, "_tdp"))
            info["cpu_tdp_w"] = int(tdp) if tdp is not None and tdp.is_integer() else tdp
            generic = safe_getattr(item, "_is_generic_tdp")
            info["cpu_tdp_is_generic"] = generic if isinstance(generic, bool) else None
        elif "GPU" in names:
            info["gpu_method"] = _gpu_method(item)
        elif "RAM" in names:
            forced = safe_getattr(item, "_force_ram_power")
            info["ram_method"] = "constant" if forced is not None else "estimate"
    return info


def private_carbon_intensity(tracker: Any) -> float | None:
    """g CO2e / kWh from CodeCarbon's emissions engine, ``None`` if unavailable."""
    emissions = safe_getattr(tracker, "_emissions")
    geo = safe_getattr(tracker, "_geo")
    if emissions is None or geo is None:
        return None
    try:
        from codecarbon.core.units import Energy

        kg = emissions.get_private_infra_emissions(Energy.from_energy(kWh=1.0), geo)
    except Exception:  # noqa: BLE001
        return None
    value = num(kg)
    return round(value * 1000.0, 4) if value is not None else None


def observed_carbon_intensity(data: Any) -> float | None:
    """g CO2e / kWh as emissions / energy of ``data`` (when energy > 0)."""
    energy = num(safe_getattr(data, "energy_consumed"))
    emissions = num(safe_getattr(data, "emissions"))
    if energy is None or emissions is None or energy <= 0:
        return None
    return round(emissions / energy * 1000.0, 4)


def extract_location(tracker: Any, data: Any, config: Mapping[str, Any]) -> dict[str, Any]:
    """The ``started.location`` object."""
    geo = safe_getattr(tracker, "_geo")
    forced = config.get("force_carbon_intensity_g_co2e_kwh")
    intensity = num(forced)
    if intensity is None:
        intensity = private_carbon_intensity(tracker)
    if intensity is None:
        intensity = observed_carbon_intensity(data)
    return {
        "country_name": text(safe_getattr(data, "country_name"))
        or text(safe_getattr(geo, "country_name")),
        "country_iso_code": text(safe_getattr(data, "country_iso_code"))
        or text(safe_getattr(geo, "country_iso_code"))
        or config.get("country_iso_code"),
        "region": text(safe_getattr(data, "region"))
        or text(safe_getattr(geo, "region"))
        or config.get("region"),
        "carbon_intensity_g_per_kwh": intensity,
    }


def _instant_power(tracker: Any, attr: str, fallback: Any) -> float | None:
    value = num(safe_getattr(tracker, attr, "W"))
    return value if value is not None else num(fallback)


def instantaneous_power(tracker: Any, data: Any) -> dict[str, float | None]:
    """Power of the last sample (``tracker._cpu_power`` ...), falling back to run averages."""
    cpu = _instant_power(tracker, "_cpu_power", safe_getattr(data, "cpu_power"))
    gpu = _instant_power(tracker, "_gpu_power", safe_getattr(data, "gpu_power"))
    ram = _instant_power(tracker, "_ram_power", safe_getattr(data, "ram_power"))
    return {"cpu": cpu, "gpu": gpu, "ram": ram, "total": total(cpu, gpu, ram)}


def average_power(data: Any) -> dict[str, float | None]:
    """Run-average power, as CodeCarbon reports it in ``EmissionsData``."""
    cpu = num(safe_getattr(data, "cpu_power"))
    gpu = num(safe_getattr(data, "gpu_power"))
    ram = num(safe_getattr(data, "ram_power"))
    return {"cpu": cpu, "gpu": gpu, "ram": ram, "total": total(cpu, gpu, ram)}


def energy(data: Any) -> dict[str, float | None]:
    cpu = num(safe_getattr(data, "cpu_energy"))
    gpu = num(safe_getattr(data, "gpu_energy"))
    ram = num(safe_getattr(data, "ram_energy"))
    consumed = num(safe_getattr(data, "energy_consumed"))
    return {
        "cpu": cpu,
        "gpu": gpu,
        "ram": ram,
        "total": consumed if consumed is not None else total(cpu, gpu, ram),
    }


def measurement_payload(
    tracker: Any, data: Any, fallback_intensity: float | None
) -> dict[str, Any]:
    """The fields of a ``measurement`` event (without ``event`` / ``ts``)."""
    intensity = observed_carbon_intensity(data)
    return {
        "duration_s": num(safe_getattr(data, "duration")),
        "power_w": instantaneous_power(tracker, data),
        "energy_kwh": energy(data),
        "emissions_kg": num(safe_getattr(data, "emissions")),
        "emissions_rate_kg_s": num(safe_getattr(data, "emissions_rate")),
        "cpu_utilization_percent": num(safe_getattr(data, "cpu_utilization_percent")),
        "ram_used_gb": num(safe_getattr(data, "ram_used_gb")),
        "carbon_intensity_g_per_kwh": intensity if intensity is not None else fallback_intensity,
    }


def tracker_is_running(tracker: Any) -> bool:
    """Whether ``start()`` really started the tracker.

    CodeCarbon's ``start()`` (and ``OfflineEmissionsTracker.__init__``) swallow
    exceptions, so for real trackers we look at ``_start_time``, which is only
    set by a successful start. Objects without that attribute are trusted.
    """
    try:
        if type(tracker).__module__.startswith("codecarbon"):
            return getattr(tracker, "_start_time", None) is not None
        return getattr(tracker, "_start_time", True) is not None
    except Exception:  # noqa: BLE001
        return True


# --------------------------------------------------------------------------- session


class TrackerSession:
    """One CodeCarbon run at a time: ``start`` → ``sample``* → ``stop``.

    Not thread-safe by itself: the runner serialises calls with a lock.
    """

    def __init__(
        self,
        tracker_factory: TrackerFactory = default_tracker_factory,
        csv_factory: CsvFactory = FinalCsvOutput,
        default_log_level: str = DEFAULT_LOG_LEVEL,
        country_codes: Callable[[], set[str] | None] = known_country_codes,
    ) -> None:
        self._tracker_factory = tracker_factory
        self._csv_factory = csv_factory
        self._default_log_level = default_log_level
        self._country_codes = country_codes
        self._reset()

    def _reset(self) -> None:
        self._tracker: Any = None
        self._capture: CaptureOutput | None = None
        self._csv: Any = None
        self._config: dict[str, Any] = {}
        self._run_id: str | None = None
        self._started_at: str | None = None
        self._intensity: float | None = None
        self._last_data: Any = None

    @property
    def running(self) -> bool:
        return self._tracker is not None

    @property
    def interval_secs(self) -> float:
        return float(self._config.get("interval_secs", DEFAULT_INTERVAL_SECS))

    # ----------------------------------------------------------------- start

    def start(self, raw_config: Mapping[str, Any] | None) -> dict[str, Any]:
        """Create and start a tracker; return the ``started`` payload.

        Raises :class:`StartError` (``bad_command`` / ``tracker_init_failed`` /
        ``already_running``); the session is then still idle.
        """
        if self.running:
            raise StartError(ALREADY_RUNNING, "a measurement is already running")
        config = normalize_config(raw_config, self._default_log_level)
        self._check_semantics(config)

        capture = CaptureOutput()
        csv_output = self._make_csv_output(config)
        handlers: list[Any] = [capture] + ([csv_output] if csv_output is not None else [])
        offline = config["country_iso_code"] is not None
        tracker = self._create_tracker(offline, tracker_kwargs(config, handlers), csv_output)

        self._tracker, self._capture, self._csv, self._config = (
            tracker,
            capture,
            csv_output,
            config,
        )
        self._started_at = now_iso()
        data = self._initial_flush()
        location = extract_location(tracker, data, config)
        self._intensity = location["carbon_intensity_g_per_kwh"]
        self._run_id = text(safe_getattr(tracker, "run_id")) or text(safe_getattr(data, "run_id"))
        return {
            "run_id": self._run_id,
            "started_at": self._started_at,
            "hardware": extract_hardware(tracker),
            "location": location,
            "config": public_config(config),
        }

    def _check_semantics(self, config: Mapping[str, Any]) -> None:
        country = config["country_iso_code"]
        if country is not None:
            codes = self._country_codes()
            if codes is not None and country not in codes:
                raise StartError(
                    TRACKER_INIT_FAILED, f"CodeCarbon has no data for country '{country}'"
                )
        if config["save_to_api"] and not (config["api_key"] and config["experiment_id"]):
            raise StartError(
                TRACKER_INIT_FAILED, "save_to_api requires both api_key and experiment_id"
            )

    def _make_csv_output(self, config: Mapping[str, Any]) -> Any:
        if not config["save_to_file"]:
            return None
        output_dir = config["output_dir"]
        try:
            os.makedirs(output_dir, exist_ok=True)
            return self._csv_factory(output_dir)
        except Exception as exc:  # noqa: BLE001
            raise StartError(
                TRACKER_INIT_FAILED, f"cannot use output_dir '{output_dir}': {exc}"
            ) from exc

    def _create_tracker(self, offline: bool, kwargs: dict[str, Any], csv_output: Any) -> Any:
        tracker: Any = None
        try:
            tracker = self._tracker_factory(offline, kwargs)
            if tracker is None:
                raise RuntimeError("the tracker factory returned None")
            tracker.start()
        except Exception as exc:  # noqa: BLE001
            self._abandon(tracker, csv_output)
            raise StartError(
                TRACKER_INIT_FAILED, f"could not start the CodeCarbon tracker: {exc}"
            ) from exc
        if not tracker_is_running(tracker):
            self._abandon(tracker, csv_output)
            raise StartError(
                TRACKER_INIT_FAILED,
                "CodeCarbon could not start the tracker (details on stderr / in log events)",
            )
        return tracker

    @staticmethod
    def _abandon(tracker: Any, csv_output: Any) -> None:
        """Best-effort cleanup of a tracker whose start failed (no CSV row)."""
        if csv_output is not None and hasattr(csv_output, "discard"):
            csv_output.discard()
        if tracker is not None:
            with contextlib.suppress(Exception):
                tracker.stop()

    def _initial_flush(self) -> Any:
        """Flush once right after start so geolocation / carbon intensity are known."""
        assert self._capture is not None
        try:
            self._tracker.flush()
        except Exception as exc:  # noqa: BLE001
            logger.warning("Initial CodeCarbon flush failed: %s", exc)
        _, data = self._capture.snapshot()
        self._last_data = data
        return data

    # ----------------------------------------------------------------- sample

    def sample(self) -> dict[str, Any]:
        """Flush the tracker and return a ``measurement`` payload.

        Raises :class:`SampleError` if CodeCarbon produced no new data.
        """
        if self._tracker is None or self._capture is None:
            raise SampleError("not running")
        before, _ = self._capture.snapshot()
        try:
            self._tracker.flush()
        except Exception as exc:  # noqa: BLE001
            raise SampleError(f"CodeCarbon flush failed: {exc}") from exc
        count, data = self._capture.snapshot()
        if data is None or count == before:
            raise SampleError("CodeCarbon flush produced no data (details on stderr)")
        self._last_data = data
        return measurement_payload(self._tracker, data, self._intensity)

    # ----------------------------------------------------------------- stop

    def stop(self) -> dict[str, Any]:
        """Stop the tracker and return the ``stopped.summary``. Always ends idle."""
        if self._tracker is None or self._capture is None:
            raise SampleError("not running")
        tracker, capture, csv_output = self._tracker, self._capture, self._csv
        try:
            before, _ = capture.snapshot()
            try:
                tracker.stop()  # CodeCarbon's stop() swallows its own errors
            except Exception as exc:  # noqa: BLE001
                logger.warning("CodeCarbon stop failed: %s", exc)
            count, data = capture.snapshot()
            if count == before:
                data = safe_getattr(tracker, "final_emissions_data") or self._last_data
            if csv_output is not None and hasattr(csv_output, "write"):
                csv_output.write()
            return self._summary(data, csv_output)
        finally:
            self._reset()

    def _summary(self, data: Any, csv_output: Any) -> dict[str, Any]:
        intensity = observed_carbon_intensity(data)
        return {
            "run_id": self._run_id or text(safe_getattr(data, "run_id")),
            "started_at": self._started_at,
            "stopped_at": now_iso(),
            "duration_s": num(safe_getattr(data, "duration")),
            "energy_kwh": energy(data),
            "emissions_kg": num(safe_getattr(data, "emissions")),
            "emissions_rate_kg_s": num(safe_getattr(data, "emissions_rate")),
            "avg_power_w": average_power(data),
            "carbon_intensity_g_per_kwh": intensity if intensity is not None else self._intensity,
            "output_file": text(safe_getattr(csv_output, "path")),
        }
