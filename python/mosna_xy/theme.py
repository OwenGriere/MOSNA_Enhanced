"""What every figure looks like before it says anything of its own.

The analyses used to be drawn by matplotlib and then by `plotters`, both onto
white. Keeping that is not nostalgia: these figures end up in slide decks and
in print, and a dark chart pasted onto a white page is a black rectangle.
"""

from __future__ import annotations

import xy

#: The paper.
BACKGROUND = "#ffffff"
#: Axis lines and tick labels, dark enough to read on paper.
AXIS = "#3c3c3c"
TEXT = "#111111"
#: Grid lines, present but never competing with the data.
GRID = "#e6e6e6"

#: Size of a figure when its specification names none, in pixels.
DEFAULT_WIDTH = 1200
DEFAULT_HEIGHT = 900


def theme() -> xy.Theme:
    """The shared look."""
    return xy.theme(
        background=BACKGROUND,
        plot_background=BACKGROUND,
        grid_color=GRID,
        axis_color=AXIS,
        text_color=TEXT,
    )


#: Most entries a legend can carry before it is dropped altogether.
#:
#: `xy` draws as many as fit and stops, silently. A reader given niches 0 to 63
#: of 281, with nothing saying the list was cut, is worse off than one given no
#: legend at all — and in every figure here the identifiers are written on the
#: figure itself, so nothing is actually lost.
MAX_LEGEND_ENTRIES = 40


def legend_fits(count: int) -> bool:
    """Whether a legend of `count` entries can be shown in full."""
    return count <= MAX_LEGEND_ENTRIES


#: Share of the plot's width kept clear on the right for a legend.
#:
#: `xy` has no placement that puts a legend *beside* the plot and shrinks the
#: plot to fit — every location it offers is inside. So the room is made in the
#: axis: the data occupies the left, the legend sits over the empty right, and
#: nothing is covered. This is what `bbox_to_anchor=(1.02, 1)` did in the
#: original, arrived at from the other direction.
LEGEND_SHARE = 0.30


def with_room_for_a_legend(low: float, high: float) -> tuple[float, float]:
    """Widen `[low, high]` so a legend has somewhere to go.

    A span of zero — every cell at the same coordinate, one sample — would
    otherwise produce a reserve of zero and an axis of no width, which is a
    division by zero one layer down.
    """
    span = high - low
    if span <= 0.0:
        span = max(abs(high), 1.0)
    return (low, high + span * LEGEND_SHARE)


def size(spec, width: int = DEFAULT_WIDTH, height: int = DEFAULT_HEIGHT) -> dict[str, int]:
    """The chart's dimensions, as `xy` takes them.

    Named by the specification when the figure has a shape of its own — a
    composition heatmap grows with the number of phenotypes, exactly as the
    matplotlib original did.
    """
    return {
        "width": int(spec.number("width", width)),
        "height": int(spec.number("height", height)),
    }
