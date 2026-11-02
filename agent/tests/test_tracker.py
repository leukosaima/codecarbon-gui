from __future__ import annotations

import os
from types import SimpleNamespace

import pytest

from codecarbon_agent.protocol import BAD_COMMAND, TRACKER_INIT_FAILED
from codecarbon_agent.tracker import (
    CaptureOutput,
    FinalCsvOutput,
    SampleError,
    StartError,
    extract_hardware,
    extract_location,
    instantaneous_power,
    measurement_payload,
    normalize_config,
    public_config,
    tracker_is_running,
    tracker_kwargs,
)

from .conftest import CPU, RAM, FakeCsv, FakePower, FakeTracker, fake_session, make_data

# --------------------------------------------------------------------------- config


def test_normalize_config_defaults(tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)
    config = normalize_config({})
    assert config["interval_secs"] == 3.0
    assert config["country_iso_code"] is None
    assert config["tracking_mode"] == "machine"
    assert config["project_name"] == "codecarbon-gui"
    assert config["save_to_file"] is True
    assert config["output_dir"] == os.path.abspath(str(tmp_path))
    assert config["save_to_api"] is False
    assert config["api_endpoint"] == "https://api.codecarbon.io"
    assert config["log_level"] == "warning"


def test_normalize_config_normalises_values():
    config = normalize_config(
        {
            "country_iso_code": " fra ",
            "interval_secs": 0.1,
            "log_level": "WARN",
            "tracking_mode": "Process",
            "unknown_key": 42,
        }
    )
    assert config["country_iso_code"] == "FRA"
    assert config["interval_secs"] == 0.5  # clamped
    assert config["log_level"] == "warning"
    assert config["tracking_mode"] == "process"
    assert "unknown_key" not in config


def test_empty_country_means_geolocation():
    assert normalize_config({"country_iso_code": ""})["country_iso_code"] is None


@pytest.mark.parametrize(
    "raw",
    [
        [],
        {"interval_secs": "3"},
        {"interval_secs": 0},
        {"interval_secs": True},
        {"country_iso_code": "FR"},
        {"country_iso_code": 250},
        {"tracking_mode": "galaxy"},
        {"save_to_file": "yes"},
        {"log_level": "loud"},
        {"pue": 0},
        {"force_carbon_intensity_g_co2e_kwh": -1},
    ],
)
def test_normalize_config_rejects_bad_types(raw):
    with pytest.raises(StartError) as info:
        normalize_config(raw)
    assert info.value.code == BAD_COMMAND


def test_public_config_redacts_api_key():
    config = normalize_config({"api_key": "secret"})
    assert public_config(config)["api_key"] == "***"
    assert config["api_key"] == "secret"


def test_tracker_kwargs_offline_vs_online():
    handlers = [object()]
    offline = tracker_kwargs(
        normalize_config({"country_iso_code": "USA", "region": "california", "pue": 1.2}),
        handlers,
    )
    assert offline["country_iso_code"] == "USA"
    assert offline["region"] == "california"
    assert offline["pue"] == 1.2
    assert offline["save_to_file"] is False  # the agent writes the CSV itself
    assert offline["allow_multiple_runs"] is True
    assert offline["output_handlers"] is handlers
    online = tracker_kwargs(normalize_config({"region": "california"}), handlers)
    assert "country_iso_code" not in online and "region" not in online
    assert "api_key" not in online


def test_tracker_kwargs_api():
    kwargs = tracker_kwargs(
        normalize_config({"save_to_api": True, "api_key": "k", "experiment_id": "e"}), []
    )
    assert kwargs["save_to_api"] is True
    assert kwargs["api_key"] == "k"
    assert kwargs["experiment_id"] == "e"
    assert kwargs["api_endpoint"] == "https://api.codecarbon.io"


# --------------------------------------------------------------------------- extraction


def test_extract_hardware_from_fake_tracker():
    tracker = FakeTracker(True, {"output_handlers": []})
    info = extract_hardware(tracker)
    assert info == {
        "cpu_model": "Fake CPU 9000",
        "cpu_count": 8,
        "cpu_physical_count": 4,
        "cpu_method": "cpu_load",
        "cpu_tdp_w": 28,
        "cpu_tdp_is_generic": False,
        "gpu_model": None,
        "gpu_count": 0,
        "gpu_method": None,
        "ram_total_gb": 31.12,
        "ram_method": "estimate",
        "descriptions": ["RAM()", "CPU(Cpu Load)"],
    }


def test_extract_hardware_apple_silicon_and_gpu():
    class AppleSiliconChip:
        def __init__(self, part):
            self.chip_part = part

    class GPU:
        devices = SimpleNamespace(devices=[type("NvidiaGPUDevice", (), {})()])

    tracker = SimpleNamespace(
        _conf={}, _hardware=[RAM(), AppleSiliconChip("CPU"), AppleSiliconChip("GPU")]
    )
    info = extract_hardware(tracker)
    assert info["cpu_method"] == "apple_powermetrics"
    assert info["gpu_method"] == "apple_powermetrics"
    assert info["descriptions"] == ["RAM()", "AppleSiliconChip", "AppleSiliconChip"]
    assert extract_hardware(SimpleNamespace(_hardware=[GPU()]))["gpu_method"] == "nvml"


def test_extract_hardware_survives_missing_internals():
    class Weird:
        @property
        def _conf(self):
            raise RuntimeError("boom")

        _hardware = 42  # not iterable

    for tracker in (object(), Weird()):
        info = extract_hardware(tracker)
        assert info["cpu_method"] is None
        assert info["descriptions"] == []


def test_instantaneous_power_prefers_last_sample():
    tracker = SimpleNamespace(
        _cpu_power=FakePower(7.5), _gpu_power=FakePower(0.0), _ram_power=FakePower(5.0)
    )
    power = instantaneous_power(tracker, make_data(10))
    assert power == {"cpu": 7.5, "gpu": 0.0, "ram": 5.0, "total": 12.5}


def test_instantaneous_power_falls_back_to_averages_and_drops_nan():
    tracker = SimpleNamespace(_cpu_power=FakePower(float("nan")))
    power = instantaneous_power(tracker, make_data(10, ram_power=float("inf")))
    assert power == {"cpu": 10.0, "gpu": 0.0, "ram": None, "total": 10.0}


def test_measurement_payload_shape_and_intensity():
    payload = measurement_payload(object(), make_data(3600), fallback_intensity=99.0)
    assert set(payload) == {
        "duration_s",
        "power_w",
        "energy_kwh",
        "emissions_kg",
        "emissions_rate_kg_s",
        "cpu_utilization_percent",
        "ram_used_gb",
        "carbon_intensity_g_per_kwh",
    }
    assert payload["energy_kwh"]["total"] == pytest.approx(0.015)
    assert payload["carbon_intensity_g_per_kwh"] == pytest.approx(50.0)
    zero = measurement_payload(object(), make_data(0), fallback_intensity=99.0)
    assert zero["carbon_intensity_g_per_kwh"] == 99.0


def test_extract_location_forced_intensity_and_geo_fallback():
    geo = SimpleNamespace(country_name="United States", country_iso_code="USA", region="ca")
    tracker = SimpleNamespace(_geo=geo)
    config = {"force_carbon_intensity_g_co2e_kwh": 12.0}
    location = extract_location(tracker, None, config)
    assert location == {
        "country_name": "United States",
        "country_iso_code": "USA",
        "region": "ca",
        "carbon_intensity_g_per_kwh": 12.0,
    }


def test_tracker_is_running_uses_start_time_for_codecarbon_objects():
    codecarbon_like = type("OfflineEmissionsTracker", (), {"__module__": "codecarbon.x"})
    assert not tracker_is_running(codecarbon_like())  # __init__ swallowed: no _start_time
    started = codecarbon_like()
    started._start_time = 1.0
    assert tracker_is_running(started)
    assert tracker_is_running(object())  # unknown objects are trusted


# --------------------------------------------------------------------------- session


def test_session_start_sample_stop(tmp_path):
    session, created = fake_session()
    started = session.start({"country_iso_code": "FRA", "output_dir": str(tmp_path / "new")})
    assert session.running
    assert (tmp_path / "new").is_dir()
    assert created[0].offline is True
    assert started["run_id"] == "run-123"
    assert started["hardware"]["cpu_method"] == "cpu_load"
    assert started["location"]["country_iso_code"] == "FRA"
    assert started["config"]["interval_secs"] == 3.0
    sample = session.sample()
    assert sample["power_w"]["cpu"] == 7.5
    summary = session.stop()
    assert not session.running
    assert created[0].stopped
    assert summary["run_id"] == "run-123"
    assert summary["avg_power_w"] == {"cpu": 10.0, "gpu": 0.0, "ram": 5.0, "total": 15.0}
    assert summary["output_file"].endswith("emissions.csv")
    assert len(FakeCsv.instances[-1].rows) == 1  # one CSV row per run


def test_session_without_csv(tmp_path):
    session, _ = fake_session()
    session.start({"save_to_file": False, "output_dir": str(tmp_path)})
    assert session.stop()["output_file"] is None


def test_session_rejects_unknown_country_and_incomplete_api():
    session, created = fake_session()
    for config in ({"country_iso_code": "XYZ"}, {"save_to_api": True, "api_key": "k"}):
        with pytest.raises(StartError) as info:
            session.start({**config, "save_to_file": False})
        assert info.value.code == TRACKER_INIT_FAILED
    assert created == [] and not session.running


def test_session_factory_failure_is_tracker_init_failed():
    def factory(offline, kwargs):
        raise OSError("no RAPL for you")

    from codecarbon_agent.tracker import TrackerSession

    session = TrackerSession(tracker_factory=factory, csv_factory=FakeCsv)
    with pytest.raises(StartError) as info:
        session.start({"save_to_file": False})
    assert info.value.code == TRACKER_INIT_FAILED
    assert "no RAPL for you" in info.value.message
    assert not session.running


def test_session_output_dir_not_creatable(tmp_path):
    blocker = tmp_path / "file"
    blocker.write_text("x")
    session, _ = fake_session()
    with pytest.raises(StartError) as info:
        session.start({"output_dir": str(blocker / "sub")})
    assert info.value.code == TRACKER_INIT_FAILED


def test_session_sample_raises_when_flush_produces_nothing():
    session, _ = fake_session(fail_flushes=2)
    session.start({"save_to_file": False})  # initial flush silently fails: tolerated
    with pytest.raises(SampleError):
        session.sample()
    assert session.sample()["duration_s"] is not None


def test_capture_output_counts():
    capture = CaptureOutput()
    assert capture.snapshot() == (0, None)
    capture.out("a", "b")
    capture.live_out("x", "y")
    assert capture.snapshot() == (1, "a")


def test_final_csv_output_writes_once(tmp_path):
    output = FinalCsvOutput(str(tmp_path))  # imports CodeCarbon's FileOutput
    written = []
    output._file = SimpleNamespace(out=lambda total, delta: written.append(total))
    output.write()  # nothing yet
    output.out("first", None)
    output.out("last", None)
    output.exit()
    output.write()
    assert written == ["last"]
    discarded = FinalCsvOutput(str(tmp_path))
    discarded._file = SimpleNamespace(out=lambda total, delta: written.append(total))
    discarded.out("x", None)
    discarded.discard()
    discarded.exit()
    assert written == ["last"]


def test_final_csv_output_missing_dir_raises(tmp_path):
    with pytest.raises(OSError):
        FinalCsvOutput(str(tmp_path / "missing"))


def test_hardware_classes_are_detected_by_name():
    assert CPU.__name__ == "CPU" and RAM.__name__ == "RAM"
