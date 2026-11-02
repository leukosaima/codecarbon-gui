from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

from codecarbon_agent import __version__
from codecarbon_agent.__main__ import main, selftest_verdict, version_line

AGENT_DIR = Path(__file__).resolve().parents[1]


def run_agent(*args: str, stdin: str = "", timeout: float = 60) -> subprocess.CompletedProcess:
    env = {**os.environ, "PYTHONPATH": str(AGENT_DIR)}
    return subprocess.run(
        [sys.executable, "-m", "codecarbon_agent", *args],
        input=stdin,
        capture_output=True,
        text=True,
        timeout=timeout,
        env=env,
    )


def test_version_in_process(capsys):
    assert main(["--version"]) == 0
    out = capsys.readouterr().out.strip()
    assert out == version_line()
    assert out.startswith(f"codecarbon-agent {__version__} (codecarbon ")


def test_version_subprocess():
    result = run_agent("--version")
    assert result.returncode == 0
    assert result.stdout.startswith(f"codecarbon-agent {__version__} (codecarbon 3.")


@pytest.mark.parametrize(
    "args", [["--bogus"], ["--seconds", "2"], ["--selftest", "--seconds", "-1"]]
)
def test_bad_usage_exits_2(args):
    with pytest.raises(SystemExit) as info:
        main(args)
    assert info.value.code == 2


def test_protocol_mode_eof_subprocess():
    lines = ['{"cmd":"status"}', "garbage", '{"cmd":"stop"}']
    result = run_agent(stdin="\n".join(lines) + "\n")
    assert result.returncode == 0, result.stderr
    events = [json.loads(line) for line in result.stdout.splitlines()]
    assert [e["event"] for e in events] == ["ready", "status", "error", "error"]
    assert [e.get("code") for e in events[2:]] == ["bad_command", "not_running"]


def test_protocol_mode_quit_subprocess():
    result = run_agent(stdin='{"cmd":"quit"}\n{"cmd":"status"}\n')
    assert result.returncode == 0
    assert [json.loads(line)["event"] for line in result.stdout.splitlines()] == ["ready"]


def test_selftest_verdict():
    good = [{"event": n} for n in ("ready", "started", "measurement", "stopped")]
    assert selftest_verdict(good)[0]
    assert not selftest_verdict(good[:2] + good[3:])[0]
