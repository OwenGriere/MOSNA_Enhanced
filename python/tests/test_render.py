"""Turning a queue of specifications into files on disk."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from mosna_xy import figures, render
from mosna_xy.spec import DOCUMENT_NAME, Spec


def queue_one(root: Path, document: dict, sequence: int = 0) -> Path:
    folder = root / f"{sequence:05d}-{document['kind']}"
    folder.mkdir(parents=True, exist_ok=True)
    (folder / DOCUMENT_NAME).write_text(json.dumps(document))
    return folder


def histogram(save_dir: Path, stem: str = "Niches_Histogram") -> dict:
    return {
        "kind": "histogram",
        "stem": stem,
        "save_dir": str(save_dir),
        "title": "Niches",
        "categories": ["0", "1"],
        "counts": [3, 5],
        "colours": ["#1f77b4", "#ff7f0e"],
    }


def test_every_kind_rust_can_emit_has_a_builder() -> None:
    """The two sides agree on a vocabulary. A kind added on one side and not
    the other is a figure that silently stops being drawn, which is exactly the
    failure nobody notices until the figures are needed."""
    assert set(figures.BUILDERS) == set(figures.KINDS)


def test_a_figure_is_written_in_every_requested_format(tmp_path: Path) -> None:
    out = tmp_path / "out"
    folder = queue_one(tmp_path / "queue", histogram(out))

    written = render.render_spec(Spec.load(folder / DOCUMENT_NAME), formats=("png", "html"))

    assert (out / "Niches_Histogram.png").is_file()
    assert (out / "Niches_Histogram.html").is_file()
    assert set(written) == {out / "Niches_Histogram.png", out / "Niches_Histogram.html"}


def test_only_the_requested_formats_are_written(tmp_path: Path) -> None:
    out = tmp_path / "out"
    folder = queue_one(tmp_path / "queue", histogram(out))

    render.render_spec(Spec.load(folder / DOCUMENT_NAME), formats=("png",))

    assert (out / "Niches_Histogram.png").is_file()
    assert not (out / "Niches_Histogram.html").exists()


def test_the_saving_directory_is_created(tmp_path: Path) -> None:
    out = tmp_path / "deep" / "not" / "there"
    folder = queue_one(tmp_path / "queue", histogram(out))

    render.render_spec(Spec.load(folder / DOCUMENT_NAME), formats=("png",))

    assert (out / "Niches_Histogram.png").is_file()


def test_an_unknown_kind_names_itself(tmp_path: Path) -> None:
    folder = queue_one(tmp_path / "queue", {"kind": "tea-leaves", "stem": "x", "save_dir": str(tmp_path)})
    with pytest.raises(render.RenderError, match="tea-leaves"):
        render.render_spec(Spec.load(folder / DOCUMENT_NAME))


def test_a_figure_with_nothing_to_draw_writes_nothing(tmp_path: Path) -> None:
    """An analysis that found no niches produces an empty histogram
    specification. Writing an empty PNG would put a blank tile in the gallery
    and read as a failed render rather than as an empty result."""
    out = tmp_path / "out"
    document = histogram(out) | {"categories": [], "counts": []}
    folder = queue_one(tmp_path / "queue", document)

    assert render.render_spec(Spec.load(folder / DOCUMENT_NAME)) == []
    assert not (out / "Niches_Histogram.png").exists()


def test_a_queue_is_drawn_in_the_order_it_was_written(tmp_path: Path) -> None:
    queue = tmp_path / "queue"
    out = tmp_path / "out"
    for index in range(3):
        queue_one(queue, histogram(out, stem=f"figure_{index}"), sequence=index)

    report = render.render_queue(queue, formats=("png",))

    assert report.failures == []
    assert [path.stem for path in report.written] == ["figure_0", "figure_1", "figure_2"]


def test_one_broken_figure_does_not_cost_the_others(tmp_path: Path) -> None:
    """Two hundred figures and one bad one: the run reports the failure and
    keeps the hundred and ninety-nine."""
    queue = tmp_path / "queue"
    out = tmp_path / "out"
    queue_one(queue, histogram(out, stem="good"), sequence=0)
    queue_one(queue, {"kind": "tea-leaves", "stem": "bad", "save_dir": str(out)}, sequence=1)
    queue_one(queue, histogram(out, stem="also_good"), sequence=2)

    report = render.render_queue(queue, formats=("png",))

    assert [path.stem for path in report.written] == ["good", "also_good"]
    assert len(report.failures) == 1
    assert "tea-leaves" in report.failures[0]
    assert not report.ok


def test_an_empty_queue_is_not_a_failure(tmp_path: Path) -> None:
    report = render.render_queue(tmp_path / "nothing", formats=("png",))
    assert report.written == []
    assert report.ok


def test_the_queue_reports_how_many_figures_it_holds(tmp_path: Path) -> None:
    queue = tmp_path / "queue"
    for index in range(4):
        queue_one(queue, histogram(tmp_path / "out", stem=f"f{index}"), sequence=index)
    assert len(render.pending(queue)) == 4
