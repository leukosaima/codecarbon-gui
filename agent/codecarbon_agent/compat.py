"""Workarounds for third-party behaviour that breaks a long-lived, frozen sidecar."""

from __future__ import annotations

import functools
import sys
from typing import Any


def is_frozen() -> bool:
    """Whether we run inside the PyInstaller binary."""
    return bool(getattr(sys, "frozen", False))


def patch_cpuinfo(frozen: bool | None = None) -> bool:
    """Make ``cpuinfo.get_cpu_info()`` (used by CodeCarbon) safe and cheap.

    * Frozen, py-cpuinfo queries CPUID in a ``multiprocessing`` child. On
      Linux that child is *forked* from our multi-threaded process and can
      deadlock on a lock held by another thread, while the parent busy-waits
      at 100 % CPU, so ``start`` never returns. With spawn (macOS/Windows) it
      re-executes the binary. We skip the CPUID source: the brand string still
      comes from /proc/cpuinfo, sysctl or the Windows registry.
    * Not frozen, every call runs a Python subprocess. The result is cached.

    Returns ``True`` if py-cpuinfo was found and patched.
    """
    try:
        import cpuinfo
        import cpuinfo.cpuinfo as implementation
    except Exception:  # noqa: BLE001 - not installed: nothing to patch
        return False
    if getattr(cpuinfo.get_cpu_info, "_codecarbon_agent_patched", False):
        return True
    if is_frozen() if frozen is None else frozen:
        implementation._get_cpu_info_from_cpuid = lambda: {}

    cached = functools.lru_cache(maxsize=1)(implementation.get_cpu_info)

    @functools.wraps(implementation.get_cpu_info)
    def get_cpu_info() -> dict[str, Any]:
        return dict(cached())

    get_cpu_info._codecarbon_agent_patched = True  # type: ignore[attr-defined]
    implementation.get_cpu_info = get_cpu_info
    cpuinfo.get_cpu_info = get_cpu_info
    return True
