"""The mean assortativity across samples, with its uncertainty.

Colour is the mean, the size of the square is the standard error. Both on one
grid, which is the whole reason this is one figure and not two: a strong mean
measured once and a weak mean measured forty times must not look alike.
"""

from __future__ import annotations

import numpy as np
import xy

from mosna_xy import theme
from mosna_xy.figures.matrix import MAX_LABELS, _axis, _colormap, _domain
from mosna_xy.spec import Spec

KIND = "assortativity_mean_std"


def build(spec: Spec) -> xy.Chart | None:
    z = spec.array("z")
    if z.ndim != 2 or z.size == 0:
        return None

    z = z[::-1]
    sizes = spec.array("sizes")
    sizes = sizes[::-1] if sizes.shape == z.shape else np.full(z.shape, 10.0)

    rows, cols = z.shape
    grid_x, grid_y = np.meshgrid(np.arange(cols), np.arange(rows))

    # A pair that was never measured has no square at all: there is no mean to
    # colour it with, and a zero-sized marker says exactly that.
    measured = np.isfinite(z)

    labels = spec.strings("labels")
    return xy.chart(
        xy.scatter(
            grid_x[measured].astype(float),
            grid_y[measured].astype(float),
            color=z[measured],
            colormap=_colormap(spec),
            color_domain=_domain(spec),
            size=sizes[measured],
            symbol="square",
            opacity=1.0,
            name="mean",
        ),
        _axis(xy.x_axis, labels, cols, tick_label_angle=-45.0),
        _axis(xy.y_axis, list(reversed(labels)), rows),
        xy.colorbar(title="mean z-score"),
        theme.theme(),
        title=spec.text("title"),
        **theme.size(spec, width=1400, height=1100),
    )
