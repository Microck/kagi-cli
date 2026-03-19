#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

: "${KAGI_SESSION_TOKEN:?set KAGI_SESSION_TOKEN before running this demo}"

export DEMO_SESSION_TOKEN="$KAGI_SESSION_TOKEN"
unset KAGI_SESSION_TOKEN
unset KAGI_API_TOKEN

cargo build --quiet
mkdir -p /tmp/kagi-demo-bin
ln -sf "$PWD/target/debug/kagi" /tmp/kagi-demo-bin/kagi
export PATH="/tmp/kagi-demo-bin:$PATH"

WORKDIR=$(mktemp -d /tmp/kagi-auth-demo.XXXXXX)
export DEMO_AUTH_WORKDIR="$WORKDIR"

cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

python3 - <<'PY'
import errno
import os
import pty
import re
import select
import subprocess
import sys
import time

token = os.environ["DEMO_SESSION_TOKEN"]
workdir = os.environ["DEMO_AUTH_WORKDIR"]
env = os.environ.copy()
env.pop("KAGI_SESSION_TOKEN", None)
env.pop("KAGI_API_TOKEN", None)

master_fd, slave_fd = pty.openpty()
process = subprocess.Popen(
    ["kagi", "auth"],
    cwd=workdir,
    stdin=slave_fd,
    stdout=slave_fd,
    stderr=slave_fd,
    env=env,
    close_fds=True,
)
os.close(slave_fd)

ansi = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]")
buffer = ""
steps = [
    ("Choose your setup path", "\n", 0.5),
    ("Paste your Session Link or raw session token", token + "\n", 0.4),
]
step_index = 0

while True:
    ready, _, _ = select.select([master_fd], [], [], 0.1)
    if master_fd in ready:
        try:
            chunk = os.read(master_fd, 4096)
        except OSError as error:
            if error.errno == errno.EIO:
                break
            raise
        if not chunk:
            break
        sys.stdout.buffer.write(chunk)
        sys.stdout.buffer.flush()
        buffer += ansi.sub("", chunk.decode("utf-8", "ignore"))

        if step_index < len(steps) and steps[step_index][0] in buffer:
            _, payload, pause = steps[step_index]
            time.sleep(pause)
            os.write(master_fd, payload.encode("utf-8"))
            buffer = ""
            step_index += 1

    if process.poll() is not None and master_fd not in ready:
        break

exit_code = process.wait()
os.close(master_fd)
if exit_code != 0:
    raise SystemExit(exit_code)

time.sleep(1.8)
PY
