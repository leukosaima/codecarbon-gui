# -*- mode: python ; coding: utf-8 -*-
# PyInstaller spec for the one-file `codecarbon-agent` sidecar.
# Build with `python build.py` (which also copies the binary for Tauri).

from PyInstaller.utils.hooks import collect_all, collect_submodules, copy_metadata

# The dashboard (dash/plotly) and the `codecarbon` CLI (typer/questionary) are
# never used by the agent; leaving them out keeps the binary small.
UNUSED_SUBPACKAGES = ("codecarbon.viz", "codecarbon.cli")

datas, binaries, hiddenimports = collect_all(
    "codecarbon",
    include_py_files=False,
    filter_submodules=lambda name: not name.startswith(UNUSED_SUBPACKAGES),
)
datas += copy_metadata("codecarbon")  # importlib.metadata.version("codecarbon")
hiddenimports += [
    "codecarbon_agent",
    "codecarbon_agent.protocol",
    "codecarbon_agent.runner",
    "codecarbon_agent.tracker",
    # protocol v2 (dashboard login), imported lazily by the runner
    "codecarbon_agent.auth",
    "codecarbon_agent.dashboard",
    "codecarbon.core.api_client",
    "codecarbon.core.schemas",
    "requests",
    "certifi",
    "authlib.common.security",
    "authlib.integrations.requests_client",
    "authlib.oauth2.rfc7636",
    "cryptography",
    # imported lazily by CodeCarbon
    "codecarbon.output_methods.file",
    "codecarbon.core.resource_tracker",
    "codecarbon.core.units",
    "pynvml",
    "psutil",
    "cpuinfo",
]
# joserfc registers its JWS / JWK algorithms (RSA, EC, OKP: `cryptography`) from submodules.
hiddenimports += collect_submodules("joserfc")

a = Analysis(
    ["pyinstaller_entry.py"],
    pathex=["."],
    binaries=binaries,
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        "codecarbon.viz",
        "codecarbon.cli",
        "dash",
        "plotly",
        "matplotlib",
        "tkinter",
        "IPython",
        "jupyter_client",
        "notebook",
        "pytest",
        "typer",
        "questionary",
        # only reached through optional / debug code paths (pydantic, pandas)
        "rich",
        "pygments",
        "markdown_it",
        "setuptools",
        "pkg_resources",
    ],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name="codecarbon-agent",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=True,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
