"""What each niche is made of."""

from __future__ import annotations

import xy

from mosna_xy.figures import matrix
from mosna_xy.spec import Spec

KIND = "niche_composition"


def build(spec: Spec) -> xy.Chart | None:
    return matrix.build(spec, colorbar_title=spec.text("colorbar_title", "proportion"))
