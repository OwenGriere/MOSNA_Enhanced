"""What every figure owes, and what each one owes on its own."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest

from mosna_xy import figures, render

#: A diverging map, standing in for the resampled `RdBu_r` Rust sends.
DIVERGING = ["#053061", "#f7f7f7", "#67001f"]


def network(make_spec, **overrides):
    body = {
        "arrays": {
            "coords": (np.array([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]), "f64"),
            "edges": (np.array([[0, 1], [1, 2]]), "u32"),
            "phenotype_index": (np.array([0, 1, 0]), "u32"),
        },
        "phenotypes": ["T cell", "B cell"],
        "colours": ["#1f77b4", "#ff7f0e"],
        "title": "Tysserand network patient 1",
    }
    return make_spec("network", stem="net_1-1", **(body | overrides))


def abundance(make_spec, **overrides):
    body = {
        "arrays": {"values": (np.array([[0.6, 0.4], [0.4, 0.6]]), "f64")},
        "samples": ["1-1", "2-1"],
        "phenotypes": ["T cell", "B cell"],
        "colours": ["#1f77b4", "#aec7e8"],
        "title": "Phenotype abundance",
    }
    return make_spec("abundance", stem="abundance", **(body | overrides))


def matrix_spec(make_spec, kind, stem, **overrides):
    body = {
        "arrays": {"z": (np.array([[1.0, -2.0], [np.nan, 0.5]]), "f64")},
        "x_labels": ["1-1", "2-1"],
        "y_labels": ["A - B", "A - C"],
        "colormap": DIVERGING,
        "domain": [-2.0, 2.0],
        "title": "Assortativity heatmap by images",
    }
    return make_spec(kind, stem=stem, **(body | overrides))


def mean_std(make_spec, **overrides):
    body = {
        "arrays": {
            "z": (np.array([[1.0, -1.0], [-1.0, 2.0]]), "f64"),
            "sizes": (np.array([[18.0, 6.0], [6.0, 12.0]]), "f64"),
        },
        "labels": ["A", "B"],
        "colormap": DIVERGING,
        "domain": [-2.0, 2.0],
        "title": "Mean assortativity + std accross samples",
    }
    return make_spec("assortativity_mean_std", stem="Assortativity_heatmap_across_patient", **(body | overrides))


def embedding(make_spec, **overrides):
    body = {
        "arrays": {
            "points": (np.array([[0.0, 0.0], [1.0, 1.0], [2.0, 0.5]]), "f64"),
            "clusters": (np.array([0, 1, 1]), "u32"),
        },
        "cluster_ids": ["0", "1"],
        "colours": ["#1f77b4", "#ff7f0e"],
        "centroids": [[0.0, 0.0], [1.5, 0.75]],
        "title": "Clusters",
    }
    return make_spec("embedding", stem="cluster_labels", **(body | overrides))


def histogram(make_spec, **overrides):
    body = {
        "categories": ["0", "1"],
        "counts": [3, 5],
        "colours": ["#1f77b4", "#ff7f0e"],
        "title": "Niches Histogram",
    }
    return make_spec("histogram", stem="Niches_Histogram", **(body | overrides))


#: One builder for one specification, for the checks every figure owes.
EVERY_FIGURE = {
    "network": network,
    "abundance": abundance,
    "assortativity_heatmap": lambda m: matrix_spec(m, "assortativity_heatmap", "Assortativity_heatmap_with_dendrogram"),
    "mixing_matrix": lambda m: matrix_spec(m, "mixing_matrix", "heatmap_zscore_1-1"),
    "niche_composition": lambda m: matrix_spec(m, "niche_composition", "Niches_Aggregated_Composition_total"),
    "assortativity_mean_std": mean_std,
    "embedding": embedding,
    "histogram": histogram,
}


@pytest.mark.parametrize("kind", sorted(EVERY_FIGURE))
def test_every_figure_is_drawn_and_exported(kind, make_spec, tmp_path: Path) -> None:
    spec = EVERY_FIGURE[kind](make_spec)
    written = render.render_spec(spec, formats=("png", "html"))

    assert [path.suffix for path in written] == [".png", ".html"]
    for path in written:
        assert path.stat().st_size > 0, f"{path.name} is empty"


@pytest.mark.parametrize("kind", sorted(EVERY_FIGURE))
def test_every_figure_is_a_chart_before_it_is_a_file(kind, make_spec) -> None:
    """A builder returns a chart and writes nothing: that is what lets a figure
    be tested, and what keeps the interactive HTML and the PNG the same chart."""
    spec = EVERY_FIGURE[kind](make_spec)
    chart = figures.BUILDERS[kind](spec)
    assert chart is not None
    assert not spec.save_dir.exists()


def test_a_network_without_cells_draws_nothing(make_spec) -> None:
    spec = network(
        make_spec,
        arrays={
            "coords": (np.empty((0, 2)), "f64"),
            "edges": (np.empty((0, 2)), "u32"),
            "phenotype_index": (np.empty(0), "u32"),
        },
    )
    assert figures.BUILDERS["network"](spec) is None


def test_a_network_draws_one_series_per_phenotype_and_its_edges(make_spec) -> None:
    """The legend is the point of the figure: a reader has to be able to say
    which colour is which cell type, and the edges have to sit under the
    nodes."""
    chart = figures.BUILDERS["network"](network(make_spec))
    names = [getattr(child, "name", None) for child in chart.children]

    assert "T cell" in names and "B cell" in names
    kinds = [type(child).__name__ for child in chart.children]
    assert kinds.count("Mark") >= 3


def test_an_embedding_without_points_draws_nothing(make_spec) -> None:
    spec = embedding(
        make_spec,
        arrays={"points": (np.empty((0, 2)), "f64"), "clusters": (np.empty(0), "u32")},
    )
    assert figures.BUILDERS["embedding"](spec) is None


def test_a_matrix_with_no_rows_draws_nothing(make_spec) -> None:
    spec = matrix_spec(
        make_spec,
        "mixing_matrix",
        "heatmap_zscore_1-1",
        arrays={"z": (np.empty((0, 0)), "f64")},
        x_labels=[],
        y_labels=[],
    )
    assert figures.BUILDERS["mixing_matrix"](spec) is None


def test_a_matrix_marks_its_missing_cells_rather_than_leaving_them_blank(make_spec) -> None:
    """A cell with no value must not be readable as a value. Blank is exactly
    what a z-score of zero looks like in a diverging map, so an unmeasured pair
    is drawn grey — the same distinction `cmap.set_bad` made."""
    spec = matrix_spec(make_spec, "mixing_matrix", "heatmap_zscore_1-1")
    chart = figures.BUILDERS["mixing_matrix"](spec)

    marks = [child for child in chart.children if type(child).__name__ == "Mark"]
    assert len(marks) >= 2, "the missing-value layer is not there"


def test_a_matrix_without_missing_cells_needs_no_grey_layer(make_spec) -> None:
    spec = matrix_spec(
        make_spec,
        "mixing_matrix",
        "heatmap_zscore_1-1",
        arrays={"z": (np.array([[1.0, -2.0], [0.0, 0.5]]), "f64")},
    )
    chart = figures.BUILDERS["mixing_matrix"](spec)
    marks = [child for child in chart.children if type(child).__name__ == "Mark"]
    assert len(marks) == 1


def test_the_first_row_of_a_matrix_is_drawn_at_the_top(make_spec) -> None:
    """Rust hands the rows down the page, as every table is read and as
    matplotlib's `imshow` drew them. A plotting library whose y axis grows
    upwards would otherwise silently print the figure upside down."""
    spec = matrix_spec(make_spec, "mixing_matrix", "heatmap_zscore_1-1")
    chart = figures.BUILDERS["mixing_matrix"](spec)

    axes = [child for child in chart.children if type(child).__name__ == "Axis"]
    y_axis = next(axis for axis in axes if axis.id == "y")
    assert list(y_axis.tick_labels) == ["A - C", "A - B"], "the rows were not flipped"


def test_a_dendrogram_is_drawn_when_one_is_given(make_spec) -> None:
    plain = matrix_spec(make_spec, "assortativity_heatmap", "Assortativity_heatmap_with_dendrogram")
    with_tree = matrix_spec(
        make_spec,
        "assortativity_heatmap",
        "Assortativity_heatmap_with_dendrogram",
        row_dendrogram=[[0.0, 0.0, 0.0, 1.0], [0.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 0.0]],
    )

    before = len([c for c in figures.BUILDERS["assortativity_heatmap"](plain).children if type(c).__name__ == "Mark"])
    after = len([c for c in figures.BUILDERS["assortativity_heatmap"](with_tree).children if type(c).__name__ == "Mark"])
    assert after == before + 1


def test_the_mean_and_the_error_are_read_from_one_square_each(make_spec) -> None:
    """Colour is the mean, size is the standard error: both at once, on one
    grid, which is the whole reason this figure is not two."""
    chart = figures.BUILDERS["assortativity_mean_std"](mean_std(make_spec))
    marks = [child for child in chart.children if type(child).__name__ == "Mark"]

    assert len(marks) == 1
    assert marks[0].kind == "scatter"


def test_abundance_stacks_one_band_per_phenotype(make_spec) -> None:
    chart = figures.BUILDERS["abundance"](abundance(make_spec))
    marks = [child for child in chart.children if type(child).__name__ == "Mark"]
    assert len(marks) == 2


def test_the_abundance_legend_does_not_sit_on_top_of_a_bar(make_spec) -> None:
    """Thirty-one cell types make a tall legend, and a legend drawn inside the
    plot covers the last sample's bar — the one nobody notices is being hidden.
    The original reserved space beside the plot with `bbox_to_anchor=(1.02, 1)`;
    `xy` has no outside placement, so the room is made in the axis itself."""
    chart = figures.BUILDERS["abundance"](abundance(make_spec))

    x_axis = next(
        axis for axis in chart.children if type(axis).__name__ == "Axis" and axis.id == "x"
    )
    assert x_axis.domain is not None, "the axis was left to fit the bars exactly"
    last_bar_edge = 2 - 1 + 0.5  # two samples, half a unit of bar either side
    assert x_axis.domain[1] > last_bar_edge, "there is no room for the legend"


def test_the_network_legend_does_not_sit_on_top_of_the_cells(make_spec) -> None:
    """Same reason, and it matters more here: what the legend would cover is
    the tissue."""
    chart = figures.BUILDERS["network"](network(make_spec))

    x_axis = next(
        axis for axis in chart.children if type(axis).__name__ == "Axis" and axis.id == "x"
    )
    assert x_axis.domain is not None
    # The fixture's cells span x from 0 to 1.
    assert x_axis.domain[1] > 1.0, "there is no room for the legend"
    assert x_axis.domain[0] <= 0.0, "the leftmost cells were cropped"


def test_a_network_with_one_cell_still_has_a_usable_axis(make_spec) -> None:
    """Every cell at the same place: the span is zero, and a reserve computed
    as a fraction of it would be zero too — leaving an axis of no width, which
    is a division by zero one layer down."""
    spec = network(
        make_spec,
        arrays={
            "coords": (np.array([[5.0, 5.0]]), "f64"),
            "edges": (np.empty((0, 2)), "u32"),
            "phenotype_index": (np.array([0]), "u32"),
        },
    )
    chart = figures.BUILDERS["network"](spec)

    x_axis = next(
        axis for axis in chart.children if type(axis).__name__ == "Axis" and axis.id == "x"
    )
    assert x_axis.domain[1] > x_axis.domain[0], "the axis has no width"


def test_the_embedding_legend_does_not_sit_on_top_of_a_cluster(make_spec) -> None:
    """Twenty niches make a legend as tall as the plot, and the corner it lands
    in holds points like every other corner."""
    chart = figures.BUILDERS["embedding"](embedding(make_spec))

    x_axis = next(
        axis for axis in chart.children if type(axis).__name__ == "Axis" and axis.id == "x"
    )
    assert x_axis.domain is not None
    assert x_axis.domain[1] > 2.0, "there is no room for the legend"


def test_a_legend_too_long_to_show_is_dropped_rather_than_truncated(make_spec) -> None:
    """A run at a fine clustering resolution finds hundreds of niches. `xy`
    draws as many legend entries as fit and stops — so the reader gets niches 0
    to 63 of 281, with nothing saying the list was cut. The identifiers are
    written at the centroids anyway, which is the legend that does not lie."""
    size = 200
    spec = embedding(
        make_spec,
        arrays={
            "points": (np.linspace(0.0, 1.0, size * 2).reshape(size, 2), "f64"),
            "clusters": (np.arange(size), "u32"),
        },
        cluster_ids=[str(i) for i in range(size)],
        colours=["#1f77b4"] * size,
        centroids=[[float(i), 0.0] for i in range(size)],
    )
    chart = figures.BUILDERS["embedding"](spec)

    legends = [c for c in chart.children if type(c).__name__ == "Legend"]
    assert legends and legends[0].show is False, "a legend of 200 entries was kept"


def test_the_histogram_has_no_legend_at_all(make_spec) -> None:
    """Every bar is already named on the axis beneath it. A legend repeating
    those names says nothing — and with two hundred niches `xy` shows the first
    forty of them and stops, which says something false."""
    chart = figures.BUILDERS["histogram"](histogram(make_spec))
    legends = [c for c in chart.children if type(c).__name__ == "Legend"]

    assert legends and legends[0].show is False


def test_a_legend_that_fits_is_kept(make_spec) -> None:
    chart = figures.BUILDERS["embedding"](embedding(make_spec))
    legends = [c for c in chart.children if type(c).__name__ == "Legend"]
    assert legends and legends[0].show is not False


def test_the_helper_layers_stay_out_of_the_legend(make_spec) -> None:
    """The grey layer and the tree are scaffolding, not series. A legend
    reading "no value / clustering / clustering" beside a colour bar tells the
    reader nothing and hides what the figure is about."""
    spec = matrix_spec(
        make_spec,
        "assortativity_heatmap",
        "Assortativity_heatmap_with_dendrogram",
        row_dendrogram=[[0.0, 0.0, 0.0, 1.0]],
    )
    chart = figures.BUILDERS["assortativity_heatmap"](spec)

    named = [c.name for c in chart.children if type(c).__name__ == "Mark" and c.name]
    assert named == [], f"these would show in the legend: {named}"


def test_the_edges_of_a_network_stay_out_of_the_legend(make_spec) -> None:
    """The legend names cell types. "edges" is not one of them."""
    chart = figures.BUILDERS["network"](network(make_spec))
    named = [c.name for c in chart.children if type(c).__name__ == "Mark" and c.name]

    assert sorted(named) == ["B cell", "T cell"]


def test_axis_names_are_hidden_rather_than_replaced_by_row_numbers(make_spec) -> None:
    """Six hundred phenotype pairs cannot all be written down the side of a
    figure. What must *not* happen is what a plotting library does by default:
    fall back to the row's index, so the axis reads 0, 25, 50 — numbers that
    mean nothing and that a reader will try to interpret."""
    size = 100
    spec = matrix_spec(
        make_spec,
        "assortativity_heatmap",
        "Assortativity_heatmap_with_dendrogram",
        arrays={"z": (np.zeros((size, 2)), "f64")},
        x_labels=["a", "b"],
        y_labels=[f"pair {i}" for i in range(size)],
    )
    chart = figures.BUILDERS["assortativity_heatmap"](spec)

    y_axis = next(
        axis for axis in chart.children if type(axis).__name__ == "Axis" and axis.id == "y"
    )
    assert y_axis.tick_labels is None, "too many names to show, so none are"
    assert (
        y_axis.style.get("tick_label_color") == "#00000000"
    ), "the row numbers underneath must not show either"


def test_a_title_reaches_the_chart(make_spec) -> None:
    chart = figures.BUILDERS["histogram"](histogram(make_spec, title="Something else"))
    assert chart.title == "Something else"
