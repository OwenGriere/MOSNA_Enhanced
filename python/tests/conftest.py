"""Building specifications the way Rust does, without Rust."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import numpy as np
import pytest

from mosna_xy.spec import DOCUMENT_NAME, Spec


def blob(directory: Path, key: str, values: np.ndarray, dtype: str) -> dict[str, Any]:
    """Write an array the way Rust writes one, and reference it."""
    kinds = {"f64": "<f8", "u32": "<u4"}
    (directory / f"{key}.bin").write_bytes(values.astype(kinds[dtype]).tobytes())
    return {"__blob__": f"{key}.bin", "dtype": dtype, "shape": list(values.shape)}


@pytest.fixture
def make_spec(tmp_path: Path):
    """A factory for specifications, one folder each."""
    counter = {"n": 0}

    def build(kind: str, stem: str = "figure", arrays: dict[str, Any] | None = None, **body: Any) -> Spec:
        folder = tmp_path / f"{counter['n']:05d}-{kind}"
        counter["n"] += 1
        folder.mkdir(parents=True, exist_ok=True)

        document: dict[str, Any] = {
            "kind": kind,
            "stem": stem,
            "save_dir": str(tmp_path / "out"),
        }
        for key, (values, dtype) in (arrays or {}).items():
            document[key] = blob(folder, key, np.asarray(values), dtype)
        document.update(body)

        (folder / DOCUMENT_NAME).write_text(json.dumps(document))
        return Spec.load(folder / DOCUMENT_NAME)

    return build
