"""Draws the MOSNA figures with the `xy` charting library.

Rust runs the analyses and writes one *specification* per figure — the values,
the labels, the colours, the title, the file to write. This package turns each
of those into an `xy` chart and exports it. Nothing here decides what a figure
means: the colour maps, the normalisations and the orderings are settled in
Rust, where they are pinned by the tests that came with the port.
"""

from mosna_xy.spec import Spec

__all__ = ["Spec"]
