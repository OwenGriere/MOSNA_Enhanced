"""The two-dimensional projection, coloured by niche.

Each niche's identifier is written at its centroid, which is what lets a blob
here be matched to a column of the composition heatmap.
"""

from __future__ import annotations

import xy

from mosna_xy import theme
from mosna_xy.spec import Spec

KIND = "embedding"

NODE_SIZE = 3.0


def build(spec: Spec) -> xy.Chart | None:
    points = spec.array("points")
    if points.ndim != 2 or points.shape[0] == 0 or points.shape[1] < 2:
        return None

    clusters = spec.array("clusters", dtype="u32")
    ids = spec.strings("cluster_ids")
    colours = spec.strings("colours")

    marks: list[xy.Mark] = []
    if clusters.size != len(points) or not ids:
        marks.append(xy.scatter(points[:, 0], points[:, 1], size=NODE_SIZE, name="nodes"))
    else:
        for position, identifier in enumerate(ids):
            selected = clusters == position
            if not selected.any():
                continue
            marks.append(
                xy.scatter(
                    points[selected, 0],
                    points[selected, 1],
                    color=colours[position] if position < len(colours) else None,
                    size=NODE_SIZE,
                    name=identifier,
                )
            )

    centroids = spec.array("centroids")
    annotations = []
    if centroids.ndim == 2 and centroids.shape[0] == len(ids):
        annotations = [
            xy.text(float(x), float(y), value=identifier, color=theme.TEXT)
            for identifier, (x, y) in zip(ids, centroids)
        ]

    # Room on the right for the legend, which with twenty niches is as tall as
    # the plot and would otherwise sit on top of one of them. With a legend
    # there is no room to make, and the space is better spent on the data.
    fits = theme.legend_fits(len(ids))
    low, high = float(points[:, 0].min()), float(points[:, 0].max())
    pad = max((high - low) * 0.05, 1e-6)
    domain = (
        theme.with_room_for_a_legend(low - pad, high + pad)
        if fits
        else (low - pad, high + pad)
    )

    return xy.chart(
        *marks,
        *annotations,
        xy.x_axis(domain=domain),
        xy.legend(show=fits, loc="upper right", title=spec.text("legend_title", "Niche")),
        theme.theme(),
        title=spec.text("title"),
        **theme.size(spec, width=1500, height=1200),
    )
