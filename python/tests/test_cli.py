"""The command Rust runs."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from mosna_xy import cli
from mosna_xy.spec import DOCUMENT_NAME


def queue_one(root: Path, stem: str, save_dir: Path, sequence: int = 0, kind: str = "histogram") -> None:
    folder = root / f"{sequence:05d}-{kind}"
    folder.mkdir(parents=True, exist_ok=True)
    (folder / DOCUMENT_NAME).write_text(
        json.dumps(
            {
                "kind": kind,
                "stem": stem,
                "save_dir": str(save_dir),
                "title": "Niches",
                "categories": ["0", "1"],
                "counts": [3, 5],
                "colours": ["#1f77b4", "#ff7f0e"],
            }
        )
    )


def test_rendering_a_queue_writes_its_figures_and_succeeds(tmp_path: Path, capsys) -> None:
    queue, out = tmp_path / "queue", tmp_path / "out"
    queue_one(queue, "Niches_Histogram", out)

    assert cli.main(["render", str(queue), "--formats", "png"]) == 0
    assert (out / "Niches_Histogram.png").is_file()


def test_progress_is_reported_in_the_protocol_the_interface_parses(tmp_path: Path, capsys) -> None:
    """The interface reads `[QT_PROGRESS]` off the analysis process's stdout.
    Rendering two hundred networks is a minute in which the bar must keep
    moving, or the run looks hung."""
    queue, out = tmp_path / "queue", tmp_path / "out"
    for index in range(3):
        queue_one(queue, f"figure_{index}", out, sequence=index)

    cli.main(["render", str(queue), "--formats", "png"])
    printed = capsys.readouterr().out

    assert "[QT_INFO]" in printed
    progress = [line for line in printed.splitlines() if line.startswith("[QT_PROGRESS]")]
    assert progress, "nothing reported its progress"
    assert "current=" in progress[0] and "total=3" in progress[0]
    assert progress[-1].split("current=")[1].split()[0] == "3"


def test_a_failing_figure_fails_the_command_and_says_which(tmp_path: Path, capsys) -> None:
    queue, out = tmp_path / "queue", tmp_path / "out"
    queue_one(queue, "good", out, sequence=0)
    folder = queue / "00001-tea-leaves"
    folder.mkdir(parents=True)
    (folder / DOCUMENT_NAME).write_text(
        json.dumps({"kind": "tea-leaves", "stem": "bad", "save_dir": str(out)})
    )

    assert cli.main(["render", str(queue), "--formats", "png"]) == 1
    assert "tea-leaves" in capsys.readouterr().err
    assert (out / "good.png").is_file(), "the good figure was still drawn"


def test_an_empty_queue_succeeds_quietly(tmp_path: Path) -> None:
    assert cli.main(["render", str(tmp_path / "nothing")]) == 0


def test_both_formats_are_written_by_default(tmp_path: Path) -> None:
    queue, out = tmp_path / "queue", tmp_path / "out"
    queue_one(queue, "Niches_Histogram", out)

    cli.main(["render", str(queue)])

    assert (out / "Niches_Histogram.png").is_file()
    assert (out / "Niches_Histogram.html").is_file()


def test_the_check_command_reports_the_versions_it_found(capsys) -> None:
    """Rust runs this before an analysis so that a missing or mismatched
    renderer is reported at the start, not after the computation."""
    assert cli.main(["check"]) == 0
    printed = capsys.readouterr().out

    assert "mosna-xy" in printed
    assert "xy" in printed
