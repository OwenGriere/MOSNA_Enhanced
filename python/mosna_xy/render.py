"""Turning queued specifications into files.

The queue is a directory of numbered folders, each holding one `figure.json`.
It is drawn in that order, and one figure that cannot be drawn does not cost
the rest: an analysis that produced two hundred figures and one bad
specification should hand back a hundred and ninety-nine figures and a precise
complaint, not nothing.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Sequence

from mosna_xy import figures
from mosna_xy.spec import DOCUMENT_NAME, Spec, SpecError


class RenderError(RuntimeError):
    """A figure that could not be drawn."""


#: What is written for each figure. The PNG is what the interface's gallery
#: shows; the HTML is the interactive chart, and what the report embeds.
DEFAULT_FORMATS: tuple[str, ...] = ("png", "html")


@dataclass
class Report:
    """What a pass over the queue achieved."""

    written: list[Path] = field(default_factory=list)
    failures: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return not self.failures


def pending(queue: Path | str) -> list[Path]:
    """The specifications waiting in `queue`, in the order they were written.

    Sorted by folder name, which Rust zero-pads precisely so that the ordinary
    lexicographic order is the numeric one.
    """
    queue = Path(queue)
    if not queue.is_dir():
        return []

    return [
        folder / DOCUMENT_NAME
        for folder in sorted(queue.iterdir())
        if (folder / DOCUMENT_NAME).is_file()
    ]


def _export(chart, path: Path, extension: str) -> None:
    exporters = {"png": chart.to_png, "html": chart.to_html, "svg": chart.to_svg}
    if extension not in exporters:
        raise RenderError(f"cannot write a figure as {extension!r}")
    exporters[extension](path)


def render_spec(spec: Spec, formats: Sequence[str] = DEFAULT_FORMATS) -> list[Path]:
    """Draw one figure, and write it in each format asked for."""
    builder = figures.BUILDERS.get(spec.kind)
    if builder is None:
        raise RenderError(f"no figure of kind {spec.kind!r} is known")

    chart = builder(spec)
    if chart is None:
        # Nothing to draw is a result, not a failure: an analysis that found no
        # niches has an empty histogram to show, and a blank PNG in the gallery
        # would read as a render that went wrong.
        return []

    spec.save_dir.mkdir(parents=True, exist_ok=True)
    written = []
    for extension in formats:
        path = spec.output(extension)
        _export(chart, path, extension)
        written.append(path)
    return written


def render_queue(
    queue: Path | str,
    formats: Sequence[str] = DEFAULT_FORMATS,
    on_progress: Callable[[int, int, str], None] | None = None,
) -> Report:
    """Draw everything waiting in `queue`."""
    documents = pending(queue)
    report = Report()

    for index, document in enumerate(documents):
        if on_progress is not None:
            on_progress(index, len(documents), document.parent.name)
        try:
            report.written.extend(render_spec(Spec.load(document), formats))
        except (SpecError, RenderError) as error:
            report.failures.append(f"{document.parent.name}: {error}")
        except Exception as error:  # noqa: BLE001 - one bad figure, not the run
            report.failures.append(
                f"{document.parent.name}: {type(error).__name__}: {error}"
            )

    if on_progress is not None:
        on_progress(len(documents), len(documents), "done")
    return report
