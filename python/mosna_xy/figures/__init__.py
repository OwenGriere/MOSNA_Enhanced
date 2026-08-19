"""One builder per kind of figure.

A builder takes a specification and returns the `xy` chart it describes, or
`None` when there is nothing to draw. It never writes a file: that is
`mosna_xy.render`'s business, and keeping the two apart is what lets a figure
be tested without a disk, and what guarantees the interactive HTML and the PNG
are the same chart rather than two drawings of the same data.
"""

from __future__ import annotations

from typing import Callable

from mosna_xy.figures import (
    abundance,
    assortativity_heatmap,
    embedding,
    histogram,
    mean_std,
    mixing_matrix,
    network,
    niche_composition,
)
from mosna_xy.spec import Spec

#: Every kind of figure Rust can queue. The list is the contract between the
#: two sides, and the tests check it against the builders below: a kind added
#: on one side and not the other is a figure that silently stops being drawn,
#: which is the failure nobody notices until the figures are needed.
KINDS: tuple[str, ...] = (
    "network",
    "abundance",
    "assortativity_heatmap",
    "assortativity_mean_std",
    "mixing_matrix",
    "niche_composition",
    "histogram",
    "embedding",
)

#: What draws each of them.
BUILDERS: dict[str, Callable[[Spec], object | None]] = {
    network.KIND: network.build,
    abundance.KIND: abundance.build,
    assortativity_heatmap.KIND: assortativity_heatmap.build,
    mean_std.KIND: mean_std.build,
    mixing_matrix.KIND: mixing_matrix.build,
    niche_composition.KIND: niche_composition.build,
    histogram.KIND: histogram.build,
    embedding.KIND: embedding.build,
}
