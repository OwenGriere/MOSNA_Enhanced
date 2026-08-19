"""What a specification has to survive being handed."""

from __future__ import annotations

import json
from pathlib import Path

import numpy as np
import pytest

from mosna_xy.spec import DOCUMENT_NAME, Spec, SpecError


def write(directory: Path, document: dict, blobs: dict[str, bytes] | None = None) -> Path:
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / DOCUMENT_NAME
    path.write_text(json.dumps(document))
    for name, payload in (blobs or {}).items():
        (directory / name).write_bytes(payload)
    return path


def test_a_specification_carries_what_the_renderer_dispatches_on(tmp_path: Path) -> None:
    path = write(
        tmp_path / "00000-embedding",
        {"kind": "embedding", "stem": "cluster_labels", "save_dir": str(tmp_path / "out")},
    )
    spec = Spec.load(path)

    assert spec.kind == "embedding"
    assert spec.stem == "cluster_labels"
    assert spec.save_dir == tmp_path / "out"
    assert spec.directory == path.parent


def test_a_document_missing_its_kind_is_refused_by_name(tmp_path: Path) -> None:
    path = write(tmp_path / "00000-nothing", {"stem": "x", "save_dir": "/tmp"})
    with pytest.raises(SpecError, match="kind"):
        Spec.load(path)


def test_a_blob_is_read_back_as_the_array_that_was_written(tmp_path: Path) -> None:
    values = np.array([[1.0, 2.0], [3.0, 4.0]])
    path = write(
        tmp_path / "00000-network",
        {
            "kind": "network",
            "stem": "net_1",
            "save_dir": str(tmp_path),
            "coords": {"__blob__": "coords.bin", "dtype": "f64", "shape": [2, 2]},
        },
        {"coords.bin": values.astype("<f8").tobytes()},
    )

    assert np.array_equal(Spec.load(path).array("coords"), values)


def test_an_integer_blob_keeps_its_width(tmp_path: Path) -> None:
    path = write(
        tmp_path / "00000-network",
        {
            "kind": "network",
            "stem": "net_1",
            "save_dir": str(tmp_path),
            "edges": {"__blob__": "edges.bin", "dtype": "u32", "shape": [2, 2]},
        },
        {"edges.bin": np.array([[0, 1], [1, 2]], dtype="<u4").tobytes()},
    )

    edges = Spec.load(path).array("edges", dtype="u32")
    assert edges.shape == (2, 2)
    assert edges.tolist() == [[0, 1], [1, 2]]


def test_a_small_array_may_be_spelled_out_in_the_document(tmp_path: Path) -> None:
    path = write(
        tmp_path / "00000-histogram",
        {"kind": "histogram", "stem": "h", "save_dir": str(tmp_path), "counts": [1, 2, 3]},
    )
    assert Spec.load(path).array("counts").tolist() == [1.0, 2.0, 3.0]


def test_an_absent_array_is_empty_rather_than_an_error(tmp_path: Path) -> None:
    """A figure with nothing to draw draws nothing; it does not crash the run
    that produced it, an hour of computation in."""
    path = write(tmp_path / "00000-histogram", {"kind": "histogram", "stem": "h", "save_dir": str(tmp_path)})
    assert Spec.load(path).array("counts").size == 0


def test_a_blob_whose_file_is_missing_says_which_one(tmp_path: Path) -> None:
    path = write(
        tmp_path / "00000-network",
        {
            "kind": "network",
            "stem": "net_1",
            "save_dir": str(tmp_path),
            "coords": {"__blob__": "coords.bin", "dtype": "f64", "shape": [2, 2]},
        },
    )
    with pytest.raises(SpecError, match="coords.bin"):
        Spec.load(path).array("coords")


def test_a_blob_of_the_wrong_length_is_caught_rather_than_reshaped(tmp_path: Path) -> None:
    """Half a file is not half a figure: a truncated blob reshaped to fit
    would draw a plausible picture of the wrong data."""
    path = write(
        tmp_path / "00000-network",
        {
            "kind": "network",
            "stem": "net_1",
            "save_dir": str(tmp_path),
            "coords": {"__blob__": "coords.bin", "dtype": "f64", "shape": [4, 2]},
        },
        {"coords.bin": np.array([1.0, 2.0], dtype="<f8").tobytes()},
    )
    with pytest.raises(SpecError, match="coords"):
        Spec.load(path).array("coords")


def test_labels_and_text_have_defaults(tmp_path: Path) -> None:
    path = write(
        tmp_path / "00000-histogram",
        {"kind": "histogram", "stem": "h", "save_dir": str(tmp_path), "title": "Niches"},
    )
    spec = Spec.load(path)

    assert spec.text("title") == "Niches"
    assert spec.text("subtitle") == ""
    assert spec.strings("labels") == []
    assert spec.strings("labels", ["a"]) == ["a"]
    assert spec.number("width", 800.0) == 800.0


def test_an_output_is_the_stem_under_the_saving_directory(tmp_path: Path) -> None:
    path = write(
        tmp_path / "00000-histogram",
        {"kind": "histogram", "stem": "Niches_Histogram", "save_dir": str(tmp_path / "run")},
    )
    spec = Spec.load(path)

    assert spec.output("png") == tmp_path / "run" / "Niches_Histogram.png"
    assert spec.output("html") == tmp_path / "run" / "Niches_Histogram.html"


def test_a_stem_may_not_escape_its_saving_directory(tmp_path: Path) -> None:
    """The specification is written by this application, not by a user — but a
    stem that walks out of the directory it was given would write a file
    somewhere nobody looks for it, and that is worth one line of checking."""
    path = write(
        tmp_path / "00000-histogram",
        {"kind": "histogram", "stem": "../escaped", "save_dir": str(tmp_path / "run")},
    )
    with pytest.raises(SpecError, match="stem"):
        Spec.load(path)
