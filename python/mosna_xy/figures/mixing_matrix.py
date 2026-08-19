"""One sample's phenotype-by-phenotype mixing matrix."""

from __future__ import annotations

import xy

from mosna_xy.figures import matrix
from mosna_xy.spec import Spec

KIND = "mixing_matrix"


def build(spec: Spec) -> xy.Chart | None:
    return matrix.build(spec, colorbar_title="z-score")
