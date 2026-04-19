from __future__ import annotations

import os
import sys

from nave import find_nave_bin


def _run() -> None:
    nave = find_nave_bin()

    if sys.platform == "win32":
        import subprocess

        try:
            completed_process = subprocess.run([nave, *sys.argv[1:]])
        except KeyboardInterrupt:
            sys.exit(2)

        sys.exit(completed_process.returncode)
    else:
        os.execvp(nave, [nave, *sys.argv[1:]])


if __name__ == "__main__":
    _run()