"""One sample's spatial network.

The edges are drawn first and the nodes on top of them, as the original did
with `zorder`: an edge crossing a cell must not hide the cell.
"""

from __future__ import annotations

import numpy as np
import xy

from mosna_xy import theme
from mosna_xy.spec import Spec

KIND = "network"

#: Edge colour and weight, from the original's black at eighty per cent.
EDGE_COLOUR = "#000000"
EDGE_OPACITY = 0.8
EDGE_WIDTH = 0.6

#: Node radius, in the same units `xy` sizes a marker with.
NODE_SIZE = 4.0


def build(spec: Spec) -> xy.Chart | None:
    coords = spec.array("coords")
    if coords.ndim != 2 or coords.shape[0] == 0:
        return None

    marks: list[xy.Mark] = []

    edges = spec.array("edges", dtype="u32")
    if edges.size:
        edges = edges.reshape(-1, 2)
        # An edge naming a cell that is not there is dropped rather than
        # drawn to the origin, which is what an out-of-range index would do.
        inside = (edges < len(coords)).all(axis=1)
        edges = edges[inside]
    if edges.size:
        marks.append(
            xy.segments(
                coords[edges[:, 0], 0],
                coords[edges[:, 0], 1],
                coords[edges[:, 1], 0],
                coords[edges[:, 1], 1],
                color=EDGE_COLOUR,
                opacity=EDGE_OPACITY,
                width=EDGE_WIDTH,
            )
        )

    phenotypes = spec.strings("phenotypes")
    colours = spec.strings("colours")
    index = spec.array("phenotype_index", dtype="u32")

    if not phenotypes or index.size != len(coords):
        # No phenotypes to separate: one series, one colour, still a network.
        marks.append(
            xy.scatter(coords[:, 0], coords[:, 1], size=NODE_SIZE, name="cells")
        )
    else:
        for position, phenotype in enumerate(phenotypes):
            selected = index == position
            if not selected.any():
                continue
            marks.append(
                xy.scatter(
                    coords[selected, 0],
                    coords[selected, 1],
                    color=colours[position] if position < len(colours) else None,
                    size=NODE_SIZE,
                    name=phenotype,
                )
            )

    # A margin keeps the outermost cells off the frame, and the reserve on the
    # right is where the legend goes — over empty space rather than over
    # tissue.
    fits = theme.legend_fits(len(marks))
    low, high = float(coords[:, 0].min()), float(coords[:, 0].max())
    pad = max((high - low) * 0.03, 1e-6)
    domain = (
        theme.with_room_for_a_legend(low - pad, high + pad)
        if fits
        else (low - pad, high + pad)
    )

    return xy.chart(
        *marks,
        xy.x_axis(domain=domain),
        xy.legend(show=fits, loc="upper right", title=spec.text("legend_title")),
        theme.theme(),
        title=spec.text("title"),
        **theme.size(spec, width=1800, height=1400),
    )
