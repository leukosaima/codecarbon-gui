"""PyInstaller entry point (a plain script: ``codecarbon_agent/__main__.py`` uses
relative imports and cannot be frozen directly)."""

import multiprocessing
import sys

if __name__ == "__main__":
    # A multiprocessing child re-executes this binary (spawn on macOS/Windows):
    # let it run its target instead of the protocol loop.
    multiprocessing.freeze_support()

    from codecarbon_agent.__main__ import main

    sys.exit(main())
