"""How many nodes fell into each niche."""

from __future__ import annotations

import xy

from mosna_xy import theme
from mosna_xy.spec import Spec

KIND = "histogram"


def build(spec: Spec) -> xy.Chart | None:
    """One bar per niche, in the niche's own colour.

    The colour is the point: it is the same one the niche has in the embedding
    and in the composition heatmap, so a tall bar here can be found again over
    there. It comes from the specification rather than from a palette chosen
    here, because Rust is what knows the ordering.
    """
    categories = spec.strings("categories")
    counts = spec.array("counts")
    if not categories or counts.size == 0:
        return None

    colours = spec.strings("colours")
    marks = [
        xy.bar(
            [category],
            [float(count)],
            color=colours[index] if index < len(colours) else None,
            name=category,
        )
        for index, (category, count) in enumerate(zip(categories, counts))
    ]

    return xy.bar_chart(
        *marks,
        # No legend: every bar is already named on the axis beneath it, so one
        # would only repeat those names — and past a few dozen niches `xy`
        # shows as many entries as fit and stops, which repeats them wrongly.
        xy.legend(show=False),
        theme.theme(),
        title=spec.text("title", "Niches Histogram"),
        **theme.size(spec, width=1200, height=800),
    )
