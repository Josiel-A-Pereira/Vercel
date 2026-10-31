"""Verify renderer startup and teardown from the packaged macOS executable."""

import json
import os
from pathlib import Path
import platform
import shutil
import socket
import subprocess
import sys
import tempfile


def verify_startup(binary, state, start_with_cursor):
    env = dict(os.environ, AGENT_DESKTOP_HOME=str(state))
    env.pop("AGENT_DESKTOP_SESSION", None)
    env.pop("AGENT_DESKTOP_AGENT_ID", None)

    def cli(*args):
        result = subprocess.run(
            [str(binary), *args], env=env, capture_output=True, text=True, timeout=15
        )
        value = json.loads(result.stdout)
        if result.returncode != 0 or value.get("ok") is not True:
            raise RuntimeError(f"{' '.join(args)} failed: {value}")
        return value["data"]

    flags = ["--cursor"] if start_with_cursor else []
    started = cli("session", "start", "--no-trace", *flags)
    session = started["session_id"]
    command = "session start --cursor" if start_with_cursor else "cursor-overlay enable"
    try:
        data = started if start_with_cursor else cli("--session", session, "cursor-overlay", "enable")
        sockets = list(state.glob(".cursor-overlay-*.sock"))
        if data.get("rendered") is not True or len(sockets) != 1:
            raise RuntimeError(
                f"{command} reported rendered={data.get('rendered')} with {len(sockets)} renderer sockets"
            )
        with socket.socket(socket.AF_UNIX) as renderer:
            renderer.settimeout(2)
            renderer.connect(str(sockets[0]))
    finally:
        cli("session", "end", session)
    if sockets[0].exists():
        raise RuntimeError(f"session end left the renderer socket for {command}")


def main(source):
    arch = {"arm64": "arm64", "x86_64": "x64"}[platform.machine()]
    with tempfile.TemporaryDirectory(prefix="ad-cursor-", dir="/tmp") as temporary:
        root = Path(temporary)
        binary = root / f"agent-desktop-darwin-{arch}"
        shutil.copy2(source, binary)
        verify_startup(binary, root / "start", start_with_cursor=True)
        verify_startup(binary, root / "enable", start_with_cursor=False)


if __name__ == "__main__":
    main(Path(sys.argv[1]))
