"""Reading what Rust wrote.

A specification is a folder: one `figure.json`, and beside it the binary blobs
its larger arrays were written to. Everything is read through `Spec`, so a
malformed document is refused in one place, by name, rather than surfacing
three modules later as an unhelpful `KeyError`.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence

import numpy as np

#: The name of the document inside a queued figure's folder.
DOCUMENT_NAME = "figure.json"

#: How a blob's declared type maps onto a numpy one. Little-endian explicitly:
#: the file is read on the machine that wrote it in every case that matters,
#: but "in every case that matters" is not a guarantee, and a big-endian reader
#: silently producing garbage coordinates is the worst possible failure.
DTYPES = {"f64": "<f8", "u32": "<u4"}


class SpecError(ValueError):
    """A specification that cannot be drawn from."""


@dataclass(frozen=True)
class Spec:
    """One figure to draw."""

    kind: str
    stem: str
    save_dir: Path
    body: dict[str, Any]
    directory: Path

    @classmethod
    def load(cls, path: Path | str) -> "Spec":
        """Read one `figure.json`."""
        path = Path(path)
        try:
            body = json.loads(path.read_text())
        except OSError as error:
            raise SpecError(f"cannot read {path}: {error}") from error
        except json.JSONDecodeError as error:
            raise SpecError(f"{path} is not valid JSON: {error}") from error

        if not isinstance(body, dict):
            raise SpecError(f"{path} is not a figure: expected an object")

        for field in ("kind", "stem", "save_dir"):
            if not isinstance(body.get(field), str) or not body[field]:
                raise SpecError(f"{path} has no {field}")

        stem = body["stem"]
        # A stem is a file name and nothing more. It is written by this
        # application, so this cannot currently fire — which is exactly when a
        # check is cheap, and the failure it prevents is a figure written
        # outside the directory the user is looking at.
        if Path(stem).name != stem or stem in {".", ".."}:
            raise SpecError(f"{path} has a stem that is not a file name: {stem!r}")

        return cls(
            kind=body["kind"],
            stem=stem,
            save_dir=Path(body["save_dir"]),
            body=body,
            directory=path.parent,
        )

    def array(self, key: str, dtype: str = "f64") -> np.ndarray:
        """The array at `key`: a blob beside the document, or a list inside it.

        An absent key is an empty array. A figure whose data did not survive
        the analysis draws nothing; it does not take down the run that produced
        it, an hour of computation in.
        """
        value = self.body.get(key)
        if value is None:
            return np.empty(0, dtype=DTYPES.get(dtype, "<f8"))

        if isinstance(value, dict) and "__blob__" in value:
            return self._blob(key, value)

        try:
            return np.asarray(value, dtype=DTYPES.get(dtype, "<f8"))
        except (TypeError, ValueError) as error:
            raise SpecError(f"{key} is not an array: {error}") from error

    def _blob(self, key: str, reference: dict[str, Any]) -> np.ndarray:
        name = reference.get("__blob__")
        declared = reference.get("dtype", "f64")
        shape = tuple(int(n) for n in reference.get("shape", []))

        if declared not in DTYPES:
            raise SpecError(f"{key} has an unknown type {declared!r}")

        path = self.directory / str(name)
        try:
            values = np.fromfile(path, dtype=DTYPES[declared])
        except OSError as error:
            raise SpecError(f"cannot read the blob {name} of {key}: {error}") from error

        expected = int(np.prod(shape)) if shape else values.size
        if values.size != expected:
            # Reshaping what fits would draw a plausible picture of the wrong
            # data, which is worse than not drawing one.
            raise SpecError(
                f"the blob {name} of {key} holds {values.size} values, "
                f"but its shape {shape} needs {expected}"
            )
        return values.reshape(shape) if shape else values

    def strings(self, key: str, default: Sequence[str] | None = None) -> list[str]:
        """The list of names at `key`."""
        value = self.body.get(key)
        if value is None:
            return list(default or [])
        if not isinstance(value, list):
            raise SpecError(f"{key} is not a list of names")
        return [str(item) for item in value]

    def text(self, key: str, default: str = "") -> str:
        """The string at `key`."""
        value = self.body.get(key)
        return default if value is None else str(value)

    def number(self, key: str, default: float = 0.0) -> float:
        """The number at `key`."""
        value = self.body.get(key)
        if value is None:
            return default
        try:
            return float(value)
        except (TypeError, ValueError) as error:
            raise SpecError(f"{key} is not a number: {value!r}") from error

    def flag(self, key: str, default: bool = False) -> bool:
        """The boolean at `key`."""
        value = self.body.get(key)
        return default if value is None else bool(value)

    def output(self, extension: str) -> Path:
        """Where the figure is written, in one format."""
        return self.save_dir / f"{self.stem}.{extension}"
