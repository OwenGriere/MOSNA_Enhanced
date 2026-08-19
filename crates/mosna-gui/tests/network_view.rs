//! The Network tab, driven through a real egui context with no window.
//!
//! The model's arithmetic is tested next to it, and the panel's state
//! transitions are tested in the panel. What neither reaches is the drawing
//! itself: the mesh building, the culling, the hover read-out, the legend. All
//! of that indexes into arrays sized by one thing and iterated by another, and
//! all of it runs inside the paint loop, where a mistake is a crash in front of
//! the user rather than a red test.
//!
//! egui runs headless, so it can be run here instead.

use std::path::Path;

use egui::{Event, Pos2, RawInput, Rect, Vec2};
use mosna_gui::app::{MosnaApp, ViewerTab};
use mosna_io::Table;

/// A network of `side * side` cells on a jittered grid, wired to its
/// neighbours, in the place the interface looks for step 1's output.
fn write_network(root: &Path, side: usize) {
    let net_dir = root.join("temp/net_dir_mosna");
    std::fs::create_dir_all(&net_dir).unwrap();

    let n = side * side;
    let mut xs = Vec::with_capacity(n);
    let mut ys = Vec::with_capacity(n);
    let mut phenotypes = Vec::with_capacity(n);
    let mut cd8 = Vec::with_capacity(n);
    let mut niches = Vec::with_capacity(n);

    for i in 0..n {
        let (row, column) = (i / side, i % side);
        xs.push(column as f64 + ((i * 7) % 5) as f64 * 0.05);
        ys.push(row as f64 + ((i * 11) % 5) as f64 * 0.05);
        phenotypes.push(["cancer", "immune", "stroma"][i % 3]);
        cd8.push((i % 97) as f64 / 97.0);
        niches.push((i % 4) as f64);
    }

    let nodes = Table::from_columns(vec![
        ("X_position".into(), Table::f64_array(xs)),
        ("Y_position".into(), Table::f64_array(ys)),
        ("Cluster".into(), Table::string_array(phenotypes)),
        ("CD8".into(), Table::f64_array(cd8)),
        ("niches".into(), Table::f64_array(niches)),
    ])
    .unwrap();
    mosna_io::write::write_parquet::write_parquet(
        &nodes,
        net_dir.join("nodes_patient-1_sample-1.parquet"),
    )
    .unwrap();

    // Each cell to its right-hand and lower neighbour: a lattice, which is
    // enough edges to exercise the segment mesh.
    let mut edges = Vec::new();
    for i in 0..n {
        let (row, column) = (i / side, i % side);
        if column + 1 < side {
            edges.push((i as u32, (i + 1) as u32));
        }
        if row + 1 < side {
            edges.push((i as u32, (i + side) as u32));
        }
    }
    mosna_io::write::write_parquet::write_parquet(
        &Table::from_edges(&edges).unwrap(),
        net_dir.join("edges_patient-1_sample-1.parquet"),
    )
    .unwrap();
}

/// An interface pointed at `root`, on the Network tab.
fn app(root: &Path) -> MosnaApp {
    let config = root.join("configuration.yaml");
    std::fs::write(
        &config,
        "\
Tysserand:
  Nodes directory: raw
  Patient column name: patient
  Sample column name: sample
  Extension: parquet
  X coordinates column: X_position
  Y coordinates column: Y_position
  Min neighbors: 3
Assortativity:
  Network directory: Default
  Patient column name: patient
  Sample column name: sample
  Extension: parquet
",
    )
    .unwrap();

    let mut app = MosnaApp::new(config);
    app.set_working_dir(root.to_path_buf());
    app.viewer_tab = ViewerTab::Network;
    app
}

/// A frame's worth of input, at the size of the real window.
fn input(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
        events,
        ..Default::default()
    }
}

/// Run one frame of the whole interface, exactly as `MosnaApp::ui` does.
///
/// The whole interface and not just the tab: the panels carve up the root `Ui`
/// in order, and the canvas gets whatever the other three leave. Drawing the
/// tab into a `Ui` of its own would test it at a size it never has.
fn frame(app: &mut MosnaApp, ctx: &egui::Context, events: Vec<Event>) {
    let _ = ctx.run_ui(input(events), |ui| {
        let context = ui.ctx().clone();
        mosna_gui::theme::apply(&context);
        mosna_gui::panels::modals::show(app, &context);
        mosna_gui::panels::top_bar::show(app, ui);
        mosna_gui::panels::browser::show(app, ui);
        mosna_gui::panels::parameters::show(app, ui);
        mosna_gui::panels::viewer::show(app, ui);
    });
}

/// Draw `frames` frames, with the pointer wherever `pointer` says.
///
/// A single frame is not enough: egui settles a layout over two passes, and
/// the tab's own geometry is prepared on one frame and drawn on the next.
fn draw(app: &mut MosnaApp, ctx: &egui::Context, frames: usize, pointer: Option<Pos2>) {
    for _ in 0..frames {
        let events = pointer.map(Event::PointerMoved).into_iter().collect();
        frame(app, ctx, events);
    }
}

#[test]
fn the_network_tab_draws_a_sample_and_reads_a_cell() {
    let directory = tempfile::tempdir().unwrap();
    write_network(directory.path(), 40);

    let ctx = egui::Context::default();
    let mut app = app(directory.path());

    // The first frame discovers what is in the network directory.
    draw(&mut app, &ctx, 1, None);
    assert_eq!(
        app.network.samples.len(),
        1,
        "the sample was not found: {:?}",
        app.network.error
    );

    app.network.load(0, "parquet");
    assert!(app.network.has_sample(), "{:?}", app.network.error);
    assert_eq!(app.network.n_cells(), 1600);

    // Colour by labels, inspect two columns, and put the pointer in the middle
    // of the canvas — which is where the sample is, since the camera fits it.
    app.network.toggle_layer("Cluster");
    app.network.inspected = vec!["CD8".into(), "niches".into()];
    draw(&mut app, &ctx, 3, Some(Pos2::new(600.0, 450.0)));
    assert!(app.network.error.is_none(), "{:?}", app.network.error);
    assert_eq!(
        app.network.drawn_cells(),
        1600,
        "a sample well under the budget must be drawn whole"
    );
    assert!(
        app.network.drawing_edges(),
        "sixteen hundred cells is not too crowded for edges"
    );

    // Add a measurement beside it: two views now, on one camera, one taking
    // the list legend and the other a ramp.
    app.network.toggle_layer("CD8");
    assert_eq!(app.network.layers().len(), 2);
    draw(&mut app, &ctx, 3, Some(Pos2::new(600.0, 450.0)));
    assert!(app.network.error.is_none(), "{:?}", app.network.error);
}

/// The zoom and the pan run inside the same paint loop.
#[test]
fn the_canvas_survives_being_scrolled_and_dragged() {
    let directory = tempfile::tempdir().unwrap();
    write_network(directory.path(), 20);

    let ctx = egui::Context::default();
    let mut app = app(directory.path());
    draw(&mut app, &ctx, 1, None);
    app.network.load(0, "parquet");
    app.network.toggle_layer("niches");

    // Frame it first, so there is a zoom to compare against.
    draw(&mut app, &ctx, 2, Some(Pos2::new(600.0, 450.0)));
    let fitted = app.network.zoom().expect("the sample was framed");

    for delta in [40.0, 120.0] {
        frame(
            &mut app,
            &ctx,
            vec![
                Event::PointerMoved(Pos2::new(600.0, 450.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: Vec2::new(0.0, delta),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
        assert!(app.network.error.is_none(), "{:?}", app.network.error);
    }

    let zoomed = app.network.zoom().unwrap();
    assert!(
        zoomed > fitted * 1.05,
        "the wheel did not zoom in: {fitted} then {zoomed}"
    );

    // And back out again, past where it started.
    for _ in 0..6 {
        frame(
            &mut app,
            &ctx,
            vec![
                Event::PointerMoved(Pos2::new(600.0, 450.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: Vec2::new(0.0, -120.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        );
    }
    assert!(
        app.network.zoom().unwrap() < fitted,
        "the wheel did not zoom back out"
    );
}

/// A sample past the drawing budget must still draw — thinned, not dropped,
/// and without taking the whole frame to decide that.
#[test]
fn a_sample_over_the_budget_still_draws() {
    let directory = tempfile::tempdir().unwrap();
    // 62 500 cells: over CELL_BUDGET, so the subsample is exercised, and over
    // EDGE_BUDGET at full zoom, so the edges are dropped at first and come
    // back as the view narrows.
    write_network(directory.path(), 250);

    let ctx = egui::Context::default();
    let mut app = app(directory.path());
    draw(&mut app, &ctx, 1, None);

    app.network.load(0, "parquet");
    assert_eq!(app.network.n_cells(), 62_500);
    app.network.toggle_layer("Cluster");

    let started = std::time::Instant::now();
    draw(&mut app, &ctx, 3, Some(Pos2::new(600.0, 450.0)));
    assert!(app.network.error.is_none(), "{:?}", app.network.error);

    let drawn = app.network.drawn_cells();
    assert!(drawn > 0, "nothing was drawn at all");
    assert!(
        drawn <= mosna_gui::model::network::CELL_BUDGET,
        "{drawn} cells went into the mesh, over the budget"
    );

    // The edges come too, and their count is held down by the same subsample:
    // an edge is kept only when both its ends were, so the two fall together.
    assert!(app.network.drawing_edges(), "the edges were dropped");
    let edges = app.network.drawn_edges();
    assert!(edges > 0, "the network came out as a cloud of dots");
    assert!(
        edges <= drawn * 4,
        "{edges} edges for {drawn} cells: the subsample is not holding them down"
    );

    // And the checkbox turns them off.
    app.network.show_edges = false;
    draw(&mut app, &ctx, 2, None);
    assert!(!app.network.drawing_edges());
    assert_eq!(app.network.drawn_edges(), 0);

    // Generous, because this is a debug build on whatever machine runs it.
    // It is here to catch a rebuild that went quadratic, not to measure.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "three frames took {:?}",
        started.elapsed()
    );
}

/// The tab is reached from the Viewer's tab strip like any other page.
#[test]
fn the_tab_is_one_of_the_viewers_pages() {
    let directory = tempfile::tempdir().unwrap();
    write_network(directory.path(), 10);

    let ctx = egui::Context::default();
    let mut app = app(directory.path());

    for tab in [
        ViewerTab::Images,
        ViewerTab::Network,
        ViewerTab::Log,
        ViewerTab::Documentation,
    ] {
        app.viewer_tab = tab;
        draw(&mut app, &ctx, 2, None);
    }
}

/// Four columns blended into one picture — through the real paint loop.
///
/// The tab builds one geometry, resolves four colour maps over it and blends
/// them per cell. Every one of those steps indexes a vector sized by something
/// else, which is the kind of mistake that only shows up when it crashes in
/// front of the user.
#[test]
fn four_columns_blend_into_one_picture() {
    let directory = tempfile::tempdir().unwrap();
    write_network(directory.path(), 40);

    let ctx = egui::Context::default();
    let mut app = app(directory.path());
    draw(&mut app, &ctx, 1, None);
    app.network.load(0, "parquet");

    for column in ["Cluster", "CD8", "niches", "X_position"] {
        app.network.toggle_layer(column);
    }
    assert_eq!(app.network.layers().len(), 4);

    draw(&mut app, &ctx, 3, Some(Pos2::new(600.0, 450.0)));
    assert!(app.network.error.is_none(), "{:?}", app.network.error);
    assert_eq!(app.network.drawn_cells(), 1600);

    // Dropping one of the four re-blends the rest rather than leaving the
    // colours of a column that is no longer chosen.
    app.network.toggle_layer("niches");
    draw(&mut app, &ctx, 2, Some(Pos2::new(600.0, 450.0)));
    assert_eq!(app.network.layers().len(), 3);
    assert!(app.network.error.is_none(), "{:?}", app.network.error);
    assert_eq!(app.network.drawn_cells(), 1600);
}

/// Changing one channel's ramp redraws, and does not disturb the others.
#[test]
fn changing_a_palette_redraws_without_disturbing_the_others() {
    use mosna_gui::model::network::Palette;

    let directory = tempfile::tempdir().unwrap();
    write_network(directory.path(), 20);

    let ctx = egui::Context::default();
    let mut app = app(directory.path());
    draw(&mut app, &ctx, 1, None);
    app.network.load(0, "parquet");
    app.network.toggle_layer("CD8");
    app.network.toggle_layer("niches");
    draw(&mut app, &ctx, 2, Some(Pos2::new(600.0, 450.0)));

    let others: Vec<Palette> = app.network.layers()[1..].iter().map(|l| l.palette).collect();
    app.network.set_palette(0, Palette::Purples);
    draw(&mut app, &ctx, 2, Some(Pos2::new(600.0, 450.0)));

    assert_eq!(app.network.layers()[0].palette, Palette::Purples);
    assert_eq!(
        others,
        app.network.layers()[1..]
            .iter()
            .map(|l| l.palette)
            .collect::<Vec<_>>(),
        "recolouring one channel moved another"
    );
    assert!(app.network.error.is_none(), "{:?}", app.network.error);
    assert!(app.network.drawn_cells() > 0, "the redraw drew nothing");
}

/// Adding a column must not move the picture.
///
/// A channel is a change of colour, not a change of view: the cells the reader
/// was looking at have to still be under the pointer afterwards, or every
/// added marker costs them their place.
#[test]
fn adding_a_column_leaves_the_framing_alone() {
    let directory = tempfile::tempdir().unwrap();
    write_network(directory.path(), 20);

    let ctx = egui::Context::default();
    let mut app = app(directory.path());
    draw(&mut app, &ctx, 1, None);
    app.network.load(0, "parquet");
    app.network.toggle_layer("CD8");
    draw(&mut app, &ctx, 2, Some(Pos2::new(600.0, 450.0)));
    let alone = app.network.zoom().expect("the sample was framed");

    app.network.toggle_layer("niches");
    draw(&mut app, &ctx, 2, Some(Pos2::new(600.0, 450.0)));
    let blended = app.network.zoom().expect("the sample was framed again");

    assert_eq!(
        alone, blended,
        "adding a column moved the camera: {alone} became {blended}"
    );
    assert!(app.network.error.is_none(), "{:?}", app.network.error);
}

/// Folding a side panel gives its width to the viewer, and unfolding gives it
/// back — through the real layout, since that is the only place it happens.
#[test]
fn folding_a_side_panel_widens_the_viewer() {
    let directory = tempfile::tempdir().unwrap();
    write_network(directory.path(), 20);

    let ctx = egui::Context::default();
    let mut app = app(directory.path());
    draw(&mut app, &ctx, 1, None);
    app.network.load(0, "parquet");
    app.network.toggle_layer("CD8");
    draw(&mut app, &ctx, 3, None);
    let both_open = app.network.canvas_size();
    assert!(both_open[0] > 0.0, "the canvas was never drawn");

    app.browser_folded = true;
    app.parameters_folded = true;
    draw(&mut app, &ctx, 3, None);
    let both_folded = app.network.canvas_size();

    assert!(
        both_folded[0] > both_open[0] + 100.0,
        "folding both panels gained only {} points",
        both_folded[0] - both_open[0]
    );
    assert!(app.network.error.is_none(), "{:?}", app.network.error);

    app.browser_folded = false;
    app.parameters_folded = false;
    draw(&mut app, &ctx, 3, None);
    assert!(
        (app.network.canvas_size()[0] - both_open[0]).abs() < 1.0,
        "unfolding did not give the width back"
    );
}
