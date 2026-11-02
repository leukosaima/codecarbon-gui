from __future__ import annotations

import pytest

from codecarbon_agent.compat import patch_cpuinfo


def test_patch_cpuinfo_caches_and_skips_cpuid(monkeypatch):
    cpuinfo = pytest.importorskip("cpuinfo")
    implementation = pytest.importorskip("cpuinfo.cpuinfo")
    calls = []
    monkeypatch.setattr(
        implementation, "get_cpu_info", lambda: calls.append(1) or {"brand_raw": "X"}
    )
    monkeypatch.setattr(cpuinfo, "get_cpu_info", implementation.get_cpu_info)
    monkeypatch.setattr(implementation, "_get_cpu_info_from_cpuid", lambda: {"from": "cpuid"})
    assert patch_cpuinfo(frozen=True)
    assert implementation._get_cpu_info_from_cpuid() == {}
    first = cpuinfo.get_cpu_info()
    first["mutated"] = True
    assert cpuinfo.get_cpu_info() == {"brand_raw": "X"}
    assert calls == [1]
    assert patch_cpuinfo(frozen=True)  # idempotent
