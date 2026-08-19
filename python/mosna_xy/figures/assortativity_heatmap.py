"""Every sample's z-scores, clustered on both axes."""

from __future__ import annotations

import xy

from mosna_xy.figures import matrix
from mosna_xy.spec import Spec

KIND = "assortativity_heatmap"


def build(spec: Spec) -> xy.Chart | None:
    return matrix.build(spec, colorbar_title="z-score")
