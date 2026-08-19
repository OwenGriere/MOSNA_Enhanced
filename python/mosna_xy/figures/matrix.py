"""The labelled matrices: assortativity, mixing matrices, niche composition.

Three figures, one shape. Rust hands over a matrix *already in the order it is
to be drawn* — clustered, filtered, transposed — together with the colour map
and the range it is to be read against. Nothing here decides what a cell means.
"""

from __future__ import annotations

import numpy as np
import xy

from mosna_xy import theme
from mosna_xy.spec import Spec

#: The colour of a cell with no value, and the one `cmap.set_bad` used.
#:
#: Grey, and not blank. In a diverging map blank *is* a value — it is what a
#: z-score of zero looks like — so an unmeasured pair left undrawn would read
#: as a measured pair with nothing to report.
MISSING = "#888888"

#: Past this many labels, an axis is left to choose its own ticks: forty
#: overlapping strings are less readable than none, and the figure is still
#: readable by shape.
MAX_LABELS = 80

#: How much of the plot the dendrogram band takes, as a fraction of the axis it
#: sits beside.
DENDROGRAM_BAND = 0.12


def _domain(spec: Spec) -> tuple[float, float] | None:
    bounds = spec.array("domain")
    return (float(bounds[0]), float(bounds[1])) if bounds.size == 2 else None


def _colormap(spec: Spec):
    stops = spec.strings("colormap")
    return stops if stops else "viridis"


def _axis(builder, labels: list[str], count: int, **extra):
    """One axis, with the names on it when there is room for them.

    When there is not — six hundred phenotype pairs cannot be written down the
    side of a figure — the axis goes silent rather than falling back to the
    row's index. Those numbers mean nothing, and a reader will try to
    interpret them.
    """
    if labels and len(labels) == count and count <= MAX_LABELS:
        return builder(tick_values=list(range(count)), tick_labels=labels, **extra)
    return builder(text=False, ticks=False, **extra)


def _dendrogram(spec: Spec, key: str, rows: int, cols: int) -> xy.Mark | None:
    """The tree drawn beside the matrix it ordered.

    Each segment is `[leaf0, height0, leaf1, height1]`: a position along the
    axis the tree belongs to, and a merge height already normalised to `[0, 1]`
    by Rust — which is what knows the linkage. Here it is only placed.
    """
    segments = spec.array(key)
    if segments.size == 0:
        return None
    segments = segments.reshape(-1, 4)

    leaves = segments[:, [0, 2]]
    heights = segments[:, [1, 3]]

    if key == "row_dendrogram":
        # Rows were flipped to put the first one at the top; the tree flips
        # with them, or it labels the wrong leaves.
        y = (rows - 1) - leaves
        x = (cols - 0.5) + heights * max(cols * DENDROGRAM_BAND, 0.5)
    else:
        x = leaves
        y = (rows - 0.5) + heights * max(rows * DENDROGRAM_BAND, 0.5)

    # Unnamed on purpose: the tree is scaffolding for reading the matrix, not
    # a series, and a legend entry for it displaces the ones that matter.
    return xy.segments(
        x[:, 0], y[:, 0], x[:, 1], y[:, 1],
        color=theme.AXIS,
        width=1.2,
    )


def build(spec: Spec, colorbar_title: str = "") -> xy.Chart | None:
    """The matrix, its missing cells, its tree and its scale."""
    z = spec.array("z")
    if z.ndim != 2 or z.size == 0:
        return None

    # Rust writes the rows in reading order, top first, as every table is read
    # and as `imshow` drew them. An axis that grows upwards would otherwise
    # print the figure upside down.
    z = z[::-1]
    rows, cols = z.shape
    xs = np.arange(cols)
    ys = np.arange(rows)

    marks: list[xy.Mark] = []
    if np.isnan(z).any():
        marks.append(
            xy.heatmap(
                np.where(np.isnan(z), 1.0, np.nan),
                x=xs,
                y=ys,
                colormap=[MISSING, MISSING],
                domain=(0.0, 1.0),
            )
        )

    marks.append(
        xy.heatmap(z, x=xs, y=ys, colormap=_colormap(spec), domain=_domain(spec))
    )

    for key in ("row_dendrogram", "column_dendrogram"):
        tree = _dendrogram(spec, key, rows, cols)
        if tree is not None:
            marks.append(tree)

    y_labels = spec.strings("y_labels")
    return xy.chart(
        *marks,
        _axis(xy.x_axis, spec.strings("x_labels"), cols, tick_label_angle=-45.0),
        _axis(xy.y_axis, list(reversed(y_labels)), rows),
        xy.colorbar(title=colorbar_title or spec.text("colorbar_title")),
        theme.theme(),
        title=spec.text("title"),
        **theme.size(spec, width=1600, height=1000),
    )
