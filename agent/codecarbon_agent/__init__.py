"""codecarbon-agent: the Python sidecar of CodeCarbon Desktop.

It wraps a CodeCarbon tracker and speaks the JSON-lines protocol described in
``docs/PROTOCOL.md`` over stdin (commands) and stdout (events).
"""

__version__ = "0.1.7"
PROTOCOL_VERSION = 2

__all__ = ["PROTOCOL_VERSION", "__version__"]
