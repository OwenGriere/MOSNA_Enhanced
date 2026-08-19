"""The command Rust runs.

Two sub-commands, and both are called by the analysis process rather than by a
person:

```text
python -m mosna_xy check
python -m mosna_xy render <queue> [--formats png,html]
```

Progress goes to stdout in the `[QT_INFO]` / `[QT_PROGRESS]` form the interface
already parses — the same protocol the Python implementation used and the Rust
analyses still emit. The renderer is a sub-process of the analysis, whose
stdout the interface is reading, so a figure being drawn moves the same bar
the computation moved.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Sequence

from mosna_xy import render


def _formats(value: str) -> tuple[str, ...]:
    return tuple(part.strip() for part in value.split(",") if part.strip())


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="mosna_xy", description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    commands.add_parser("check", help="report the renderer and charting versions")

    draw = commands.add_parser("render", help="draw every figure waiting in a queue")
    draw.add_argument("queue", type=Path, help="the directory Rust wrote the specifications to")
    draw.add_argument(
        "--formats",
        type=_formats,
        default=render.DEFAULT_FORMATS,
        help="comma-separated, from png, html and svg",
    )
    return parser


def _check() -> int:
    import xy

    print(f"mosna-xy {_version()}")
    print(f"xy {getattr(xy, '__version__', 'unknown')}")
    print(f"python {sys.version.split()[0]}")
    return 0


def _version() -> str:
    try:
        from importlib.metadata import version

        return version("mosna-xy")
    except Exception:  # noqa: BLE001 - a checkout that was never installed
        return "unknown"


def _render(queue: Path, formats: Sequence[str]) -> int:
    waiting = render.pending(queue)
    if not waiting:
        return 0

    print(f"[QT_INFO] Drawing {len(waiting)} figures with xy", flush=True)

    def report(current: int, total: int, name: str) -> None:
        print(
            f"[QT_PROGRESS] current={current} total={total} desc=[PROCESS] Drawing figures",
            flush=True,
        )

    report_card = render.render_queue(queue, formats=formats, on_progress=report)

    for failure in report_card.failures:
        print(f"cannot draw {failure}", file=sys.stderr)

    if report_card.ok:
        print(f"[QT_INFO] {len(report_card.written)} files written", flush=True)
        return 0
    return 1


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    if arguments.command == "check":
        return _check()
    return _render(arguments.queue, arguments.formats)
