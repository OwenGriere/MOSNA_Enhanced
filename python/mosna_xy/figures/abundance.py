"""How much of each phenotype every sample holds.

Stacked, because the question the figure answers is what a sample is *made of*
— the bands have to add up to the whole for that to be readable at a glance.
"""

from __future__ import annotations

import numpy as np
import xy

from mosna_xy import theme
from mosna_xy.spec import Spec

KIND = "abundance"


def build(spec: Spec) -> xy.Chart | None:
    values = spec.array("values")
    if values.ndim != 2 or values.size == 0:
        return None

    samples = spec.strings("samples")
    phenotypes = spec.strings("phenotypes")
    colours = spec.strings("colours")
    if len(samples) != values.shape[1] or len(phenotypes) != values.shape[0]:
        return None

    # Numeric positions rather than the sample names themselves: the axis has
    # to be widened to make room for the legend, and only a numeric axis has a
    # domain to widen.
    positions = np.arange(len(samples), dtype=float)

    # Each band starts where the ones below it ended.
    base = np.zeros(values.shape[1])
    marks = []
    for position, phenotype in enumerate(phenotypes):
        band = np.nan_to_num(values[position], nan=0.0)
        marks.append(
            xy.bar(
                positions,
                band,
                base=base.copy(),
                color=colours[position] if position < len(colours) else None,
                name=phenotype,
                width=0.8,
            )
        )
        base = base + band

    fits = theme.legend_fits(len(phenotypes))
    domain = (
        theme.with_room_for_a_legend(-0.6, len(samples) - 0.5)
        if fits
        else (-0.6, len(samples) - 0.5)
    )

    return xy.chart(
        *marks,
        xy.x_axis(
            tick_values=list(range(len(samples))),
            tick_labels=samples,
            domain=domain,
            tick_label_angle=-45.0,
        ),
        xy.legend(show=fits, loc="upper right", title=spec.text("legend_title", "Phenotype")),
        theme.theme(),
        title=spec.text("title"),
        **theme.size(spec, width=1800, height=900),
    )
