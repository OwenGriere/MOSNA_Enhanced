"""The three figures a finished sweep is read through.

They answer three different questions about the same set of runs, and a sweep
is not readable without all three:

* ``sweep_agreement`` — where along the grid does the answer actually change?
* ``sweep_matrix`` — which runs agree with which, when there is no order to
  walk along?
* ``sweep_stability`` — which niches survive the sweep, and which dissolve?

None of them is the niche count, which the comparison table already shows and
which cannot answer any of the three: a run can go from eight niches to eight
niches while reassigning half the cells.
"""

from __future__ import annotations

import numpy as np
import xy

from mosna_xy import theme
from mosna_xy.figures import matrix as matrix_figure
from mosna_xy.spec import Spec

AGREEMENT_KIND = "sweep_agreement"
MATRIX_KIND = "sweep_matrix"
STABILITY_KIND = "sweep_stability"

#: Past this many steps the pair names are dropped from the axis: forty
#: overlapping strings are less readable than none, and the shape of the line
#: is what the figure is for.
MAX_STEP_LABELS = 24


def build_agreement(spec: Spec) -> xy.Chart | None:
    """Two lines over the order the grid was walked in.

    Both measures rather than one because they part company where the niches
    are uneven — the pair count is dominated by the large ones, the information
    measure is not — and a reader who sees them diverge has learnt something a
    single line would have hidden.
    """
    ari = spec.array("ari")
    ami = spec.array("ami")
    if ari.size == 0:
        return None

    steps = np.arange(ari.size)
    pairs = spec.strings("pairs")
    # Which series is which is Rust's decision, as it is for every other
    # figure; this only draws them.
    colours = spec.strings("colours", ["#1f4e99", "#c47f17"])

    marks = [
        xy.line(steps, ari, name="ARI", color=colours[0], width=2.0),
        xy.scatter(steps, ari, color=colours[0], size=6.0, name="ARI"),
    ]
    if ami.size == ari.size:
        marks += [
            xy.line(steps, ami, name="AMI", color=colours[1], width=2.0),
            xy.scatter(steps, ami, color=colours[1], size=6.0, name="AMI"),
        ]

    if pairs and len(pairs) == ari.size and ari.size <= MAX_STEP_LABELS:
        x_axis = xy.x_axis(
            tick_values=list(range(ari.size)),
            tick_labels=pairs,
            tick_label_angle=-45.0,
            label="consecutive runs",
        )
    else:
        x_axis = xy.x_axis(label="step along the grid")

    return xy.chart(
        *marks,
        x_axis,
        # Fixed: agreement is bounded, and a scale taken from the data would
        # stretch a flat sweep into looking like a turbulent one. It starts
        # slightly below zero because the adjusted indices go negative when two
        # partitions agree less than chance.
        xy.y_axis(label="agreement", domain=(-0.1, 1.05)),
        xy.legend(show=True),
        theme.theme(),
        title=spec.text("title", "Agreement between consecutive runs"),
        **theme.size(spec, width=1400, height=700),
    )


def build_matrix(spec: Spec) -> xy.Chart | None:
    """Every run against every other, read for its blocks."""
    return _heatmap(
        spec,
        values=spec.array("values"),
        rows=spec.strings("runs"),
        columns=spec.strings("runs"),
        colorbar="adjusted Rand index",
        default_title="Agreement between every pair of runs",
    )


def build_stability(spec: Spec) -> xy.Chart | None:
    """How each niche of the reference run fares in the others.

    A row that stays bright across the sweep is a niche that is a feature of
    the tissue; one that darkens is a feature of the settings.
    """
    return _heatmap(
        spec,
        values=spec.array("values"),
        rows=spec.strings("niches"),
        columns=spec.strings("runs"),
        colorbar="best Jaccard overlap",
        default_title="Niche stability across the sweep",
    )


def _heatmap(spec, values, rows, columns, colorbar, default_title):
    """A labelled matrix, drawn the way every other matrix in MOSNA is.

    Handing the work to `matrix.build` rather than laying out a second heatmap
    here is what keeps the colour bar, the missing-cell grey and the label
    thinning identical across the figures — a reader who has learnt to read one
    of them can read this one.
    """
    if values.ndim != 2 or values.size == 0:
        return None

    proxy = Spec(
        kind=spec.kind,
        stem=spec.stem,
        save_dir=spec.save_dir,
        body={
            **spec.body,
            "title": spec.text("title", default_title),
            "colorbar_title": colorbar,
            "x_labels": list(columns),
            "y_labels": list(rows),
        },
        directory=spec.directory,
    )
    # `matrix.build` reads its matrix from `z`; the array is already loaded, so
    # it is handed over directly rather than round-tripped through a blob.
    proxy.body["z"] = spec.body["values"]
    return matrix_figure.build(proxy, colorbar_title=colorbar)
