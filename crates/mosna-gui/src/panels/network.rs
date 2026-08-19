//! The interactive network — the Viewer's Network tab.
//!
//! The arithmetic is all in [`crate::model::network`]; what is left here is the
//! drawing, the input, and the decision of when the prepared geometry has gone
//! stale.
//!
//! # How it stays fast
//!
//! A sample is tens to hundreds of thousands of cells, and egui copies every
//! vertex it is given into the frame's primitives. Two things keep that
//! bounded. The geometry is *prepared* — cell centres and edge endpoints in
//! mesh space, with their colours resolved — for a region half a screen wider
//! than the view, and re-used until the view leaves it, so panning does not
//! re-read a column or re-run the colour map. And the prepared set is capped:
//! past [`crate::model::network::CELL_BUDGET`] cells only every `stride`-th is
//! kept, which at that
//! zoom is the same picture because the cells are sub-pixel and overlapping.
//!
//! What is rebuilt every frame is the mesh itself, from the prepared centres
//! and the camera. That is a multiply and an add per vertex, and it is what
//! lets a cell keep the same size on screen at every zoom without the geometry
//! depending on the zoom.

use egui::{Color32, Mesh, Pos2, Rect, Sense, Stroke, Vec2};

use crate::app::MosnaApp;
use crate::model::network::{
    circle_segments, covers, flip, fuse, mesh_region, point_radius, stride_for, Attribute, Bounds,
    Camera, Channel, Legend, NetworkSample, Palette, Point, MAX_LAYERS,
};
use crate::theme;

/// Width of the column of controls beside the canvas.
const MARGIN_WIDTH: f32 = 250.0;
/// How tall a drop-down's list may get before it scrolls.
///
/// Four times the interface's usual, because these lists are long in a way the
/// parameter drop-downs are not: a cohort has as many patients as it has, and
/// a nodes file as many columns as the panel measured. Scrolling a list to
/// find a patient is the sort of thing that makes an interface tiring.
const LIST_HEIGHT: f32 = 4.0 * 400.0;
/// How much wider than the margin the column list opens.
///
/// A column name is written by whoever exported the file — `X coordinates
/// column for niches`, `CD8_membrane_intensity_mean` — and at the margin's
/// width they all end in an ellipsis, which is no help at all when the
/// question is which of two similarly-prefixed columns to colour by.
const COLUMN_LIST_WIDTH: f32 = 4.0;
/// A legend with more entries than this scrolls instead of pushing the canvas
/// off the panel.
const LEGEND_ROWS_BEFORE_SCROLL: usize = 14;
/// How far from a cell the pointer still counts as being on it, as a multiple
/// of the radius the cell is drawn at.
const HOVER_SLACK: f32 = 2.0;
/// One notch of the wheel.
const ZOOM_PER_SCROLL_LINE: f32 = 1.15;
/// What the zoom buttons do per press.
const ZOOM_PER_BUTTON: f32 = 1.4;

/// Everything the tab remembers between frames.
pub struct NetworkState {
    /// The samples found in the network directory, and which one is shown.
    pub samples: Vec<crate::model::browser::SampleRow>,
    pub selected: Option<usize>,
    /// The patient chosen in the first drop-down, when the dataset has two
    /// levels and the sample has not been picked yet.
    pub patient: Option<String>,
    /// Whether the edges are drawn at all. On by default: the edges are the
    /// network, and a cloud of dots is a scatter plot.
    pub show_edges: bool,
    /// The loaded sample, when one is.
    sample: Option<NetworkSample>,
    camera: Option<Camera>,

    /// The columns the cells are coloured by, blended into one colour each, in
    /// the order they were chosen. Empty draws the cells plain.
    pub layers: Vec<Layer>,

    /// Columns the tooltip prints, and their values.
    pub inspected: Vec<String>,
    inspected_values: Vec<Vec<String>>,

    /// The coordinate columns, seeded from the configuration.
    pub x_column: String,
    pub y_column: String,

    /// What went wrong, shown in place of the canvas.
    pub error: Option<String>,

    /// The middle of the canvas as the last frame drew it, so the zoom buttons
    /// have somewhere to zoom about — the wheel has the pointer, they do not.
    canvas_centre: Point,
    /// The whole canvas as the last frame drew it, which is what the side
    /// panels being folded away changes.
    canvas_size: [f32; 2],

    prepared: Option<Prepared>,
}

/// One column, its palette, and that column read.
///
/// A channel of the picture. Every layer contributes its colour to every cell,
/// weighted by how strongly that cell expresses its column — one network in
/// which four columns can be seen at once, rather than four networks.
pub struct Layer {
    pub column: String,
    pub palette: Palette,
    attribute: Option<Attribute>,
}

impl Layer {
    /// The column read, when it could be.
    ///
    /// `None` while the column names something the loaded sample does not
    /// have — a layer kept from a sample that had it, which the error line
    /// reports rather than the layer quietly disappearing.
    pub fn attribute(&self) -> Option<&Attribute> {
        self.attribute.as_ref()
    }
}

/// A drop-down of the tab's own size.
///
/// Every list in this margin is long — a cohort's patients, a nodes file's
/// columns — so they share one builder rather than each carrying the same
/// three settings.
fn list<R>(id: &str, caption: &str, ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) {
    list_of_width(id, caption, ui, 1.0, contents);
}

/// The same, with a list that opens `factor` times the width of the margin.
///
/// Only the *open* list widens. The button keeps the margin's width, because a
/// widget wider than the panel holding it makes egui grow that panel's rect to
/// contain the overflow — and the panel comes back wider on the next frame,
/// and wider again on the one after. That is the ratchet
/// [`crate::panels::field_row`] documents having hit; the popup is a layer of
/// its own and can overhang the canvas without touching the layout underneath.
fn wide_list<R>(
    id: &str,
    caption: &str,
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) {
    list_of_width(id, caption, ui, COLUMN_LIST_WIDTH, contents);
}

fn list_of_width<R>(
    id: &str,
    caption: &str,
    ui: &mut egui::Ui,
    factor: f32,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) {
    let width = ui.available_width();
    egui::ComboBox::from_id_salt(id)
        .selected_text(caption)
        .width(width)
        .height(LIST_HEIGHT)
        .wrap_mode(egui::TextWrapMode::Truncate)
        .show_ui(ui, |ui| {
            // The popup takes its width from the button; asking for more here
            // is what lets a long column name be read rather than elided.
            ui.set_min_width(width * factor);
            contents(ui)
        });
}

/// Geometry resolved for a region of the sample, re-used while the view stays
/// inside it.
struct Prepared {
    /// Cell centres in mesh space.
    cells: Vec<Point>,
    /// One colour per cell: every chosen column's contribution, blended.
    colours: Vec<Color32>,
    /// Edge endpoints in mesh space, empty when the sample has none or the
    /// user has turned them off.
    edges: Vec<(Point, Point)>,
    region: Bounds,
    /// The colouring the cells were resolved with, so a change of column or of
    /// palette is noticed.
    colouring: Vec<(String, Palette)>,
    with_edges: bool,
}

impl Default for NetworkState {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            selected: None,
            patient: None,
            // The one field that is not its type's default: a network without
            // its edges is a scatter plot.
            show_edges: true,
            sample: None,
            camera: None,
            layers: Vec::new(),
            inspected: Vec::new(),
            inspected_values: Vec::new(),
            x_column: String::new(),
            y_column: String::new(),
            error: None,
            canvas_centre: [0.0, 0.0],
            canvas_size: [0.0, 0.0],
            prepared: None,
        }
    }
}

impl NetworkState {
    /// Go back to the state the tab has when the application opens.
    ///
    /// Everything, not only the loaded sample: the chosen patient, the
    /// colouring, the hover columns, the camera. They all describe a dataset,
    /// and the two callers — a change of working directory, and Refresh — are
    /// both moments where that dataset may no longer be the same one. A
    /// colouring left pointing at a column the new files do not have is worse
    /// than an empty drop-down.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The columns of the loaded sample, for the pickers.
    pub fn columns(&self) -> &[String] {
        self.sample.as_ref().map(|s| &s.columns[..]).unwrap_or(&[])
    }

    pub fn n_cells(&self) -> usize {
        self.sample
            .as_ref()
            .map(NetworkSample::n_cells)
            .unwrap_or(0)
    }

    pub fn has_sample(&self) -> bool {
        self.sample.is_some()
    }

    /// How many cells the last frame actually put on the canvas.
    ///
    /// Fewer than [`Self::n_cells`] when the view is wide enough for the
    /// subsample to bite. Worth saying out loud: a reader counting dots should
    /// know when they are not all there.
    pub fn drawn_cells(&self) -> usize {
        self.prepared
            .as_ref()
            .map(|prepared| prepared.cells.len())
            .unwrap_or(0)
    }

    /// Whether the edges are being drawn.
    pub fn drawing_edges(&self) -> bool {
        self.prepared
            .as_ref()
            .map(|prepared| prepared.with_edges)
            .unwrap_or(false)
    }

    /// How many edges the last frame put on the canvas.
    pub fn drawn_edges(&self) -> usize {
        self.prepared
            .as_ref()
            .map(|prepared| prepared.edges.len())
            .unwrap_or(0)
    }

    /// How far the camera is zoomed in, or `None` before the first frame has
    /// framed the sample.
    /// The space the tab's canvas was given by the last frame.
    pub fn canvas_size(&self) -> [f32; 2] {
        self.canvas_size
    }

    pub fn zoom(&self) -> Option<f32> {
        self.camera.map(|camera| camera.scale)
    }

    /// Load the sample at `row`, replacing whatever was shown.
    pub fn load(&mut self, row: usize, extension: &str) {
        let Some(sample) = self.samples.get(row) else {
            return;
        };

        let extension = match mosna_io::read::get_opener::Extension::parse(extension.trim()) {
            Ok(extension) => extension,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };

        // The edges file sits beside the nodes file under the name the whole
        // toolchain uses. `None` when step 1 has not run: the cells still draw.
        let edges = sample.edges_file.as_ref().map(|name| {
            sample
                .nodes_path
                .parent()
                .unwrap_or(&sample.nodes_path)
                .join(name)
        });

        match NetworkSample::load(
            &sample.nodes_path,
            edges.as_deref(),
            extension,
            self.x_column.trim(),
            self.y_column.trim(),
        ) {
            Ok(loaded) => {
                self.selected = Some(row);
                self.sample = Some(loaded);
                self.camera = None;
                self.prepared = None;
                self.inspected_values.clear();
                self.error = None;
                self.refresh_layers();
                self.refresh_inspected();
            }
            Err(error) => {
                self.sample = None;
                self.prepared = None;
                self.error = Some(error.to_string());
            }
        }
    }

    /// The channels, in the order they were chosen.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Add a column as a channel, or drop the channel that already shows it.
    ///
    /// Refused past [`MAX_LAYERS`] rather than silently dropping the oldest:
    /// the picker greys out what it will not accept, and a click that is
    /// refused there should do nothing at all rather than something the user
    /// did not ask for.
    pub fn toggle_layer(&mut self, column: &str) {
        match self.layers.iter().position(|layer| layer.column == column) {
            Some(index) => {
                self.layers.remove(index);
            }
            None if self.layers.len() < MAX_LAYERS => {
                // The first palette no channel is already using, so a new
                // channel's colour is its own even after one in the middle has
                // been removed.
                let palette = Palette::ALL
                    .into_iter()
                    .find(|palette| !self.layers.iter().any(|layer| layer.palette == *palette))
                    .unwrap_or_default();
                self.layers.push(Layer {
                    column: column.to_string(),
                    palette,
                    attribute: None,
                });
            }
            None => return,
        }
        self.refresh_layers();
    }

    /// Draw one channel with another ramp.
    pub fn set_palette(&mut self, index: usize, palette: Palette) {
        let Some(layer) = self.layers.get_mut(index) else {
            return;
        };
        if layer.palette != palette {
            layer.palette = palette;
            // The colours are resolved into the prepared geometry, so the
            // picture does not change until that is thrown away.
            self.prepared = None;
        }
    }

    /// Re-read every channel's column.
    fn refresh_layers(&mut self) {
        self.prepared = None;
        let Some(sample) = &self.sample else { return };

        let mut missing: Option<String> = None;
        for layer in &mut self.layers {
            match sample.attribute(&layer.column) {
                Ok(attribute) => layer.attribute = Some(attribute),
                Err(error) => {
                    layer.attribute = None;
                    missing.get_or_insert_with(|| error.to_string());
                }
            }
        }
        // One line, for the first column that could not be read: four
        // channels over a sample that has none of their columns would
        // otherwise report the same thing four times over, and only the last
        // would be seen.
        self.error = missing;
    }

    /// Re-read the columns the tooltip prints.
    fn refresh_inspected(&mut self) {
        let Some(sample) = &self.sample else { return };
        self.inspected_values = self
            .inspected
            .iter()
            .map(|column| sample.text_column(column).unwrap_or_default())
            .collect();
    }
}

/// Draw the tab.
pub fn show(app: &mut MosnaApp, ui: &mut egui::Ui) {
    if app.network.samples.is_empty() && app.network.error.is_none() {
        discover(app);
    }

    // The controls first: they are what makes the canvas mean anything, and
    // the canvas takes whatever is left.
    egui::containers::Panel::right("network_controls")
        .default_size(MARGIN_WIDTH)
        .size_range(200.0..=420.0)
        .frame(
            egui::Frame::new()
                .fill(theme::SURFACE)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .corner_radius(egui::CornerRadius::same(4)),
        )
        .show(ui, |ui| controls(app, ui));

    if let Some(error) = app.network.error.clone() {
        ui.centered_and_justified(|ui| {
            ui.label(egui::RichText::new(error).color(theme::LOG_ERROR));
        });
        return;
    }

    if !app.network.has_sample() {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(
                    "Choose a sample to draw its network.\n\
                     Run step 1 first if the list is empty.",
                )
                .color(theme::TEXT_MUTED),
            );
        });
        return;
    }

    canvas(app, ui);
}

/// Find the samples of the network directory.
fn discover(app: &mut MosnaApp) {
    seed_coordinate_columns(app);
    match app.browser.discover_networks() {
        Ok(rows) => {
            app.network.samples = rows;
            app.network.error = None;
        }
        Err(error) => app.network.error = Some(error.to_string()),
    }
}

/// Take the coordinate columns from the configuration, once.
///
/// The niche section may name its own pair — a re-plot can be drawn against
/// different coordinates — and falls back to step 1's.
fn seed_coordinate_columns(app: &mut MosnaApp) {
    if !app.network.x_column.is_empty() {
        return;
    }
    let text = |section: &str, key: &str| {
        app.config
            .get(section, key)
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or_default()
            .to_string()
    };

    let pick = |niche: String, tysserand: String, fallback: &str| {
        if !niche.trim().is_empty() {
            niche
        } else if !tysserand.trim().is_empty() {
            tysserand
        } else {
            fallback.to_string()
        }
    };

    app.network.x_column = pick(
        text("Niche Analysis", "X coordinates column for niches"),
        text("Tysserand", "X coordinates column"),
        "X",
    );
    app.network.y_column = pick(
        text("Niche Analysis", "Y coordinates column for niches"),
        text("Tysserand", "Y coordinates column"),
        "Y",
    );
}

// ---------------------------------------------------------------------------
// The margin
// ---------------------------------------------------------------------------

fn controls(app: &mut MosnaApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .id_salt("network_controls_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            sample_picker(app, ui);
            ui.add_space(8.0);
            colour_picker(app, ui);
            ui.add_space(8.0);
            tooltip_picker(app, ui);
            ui.add_space(10.0);
            ui.separator();
            legend(app, ui);
        });
}

/// Whether the dataset names a sample as well as a patient.
///
/// Read off the discovered files rather than off the configuration: the
/// configuration says how to *parse* a file name, and this asks what the names
/// on disk actually turned out to carry.
fn has_two_levels(samples: &[crate::model::browser::SampleRow]) -> bool {
    samples.iter().any(|sample| sample.sample.is_some())
}

/// The patients, deduplicated, in the order the files were found.
fn patients_of(samples: &[crate::model::browser::SampleRow]) -> Vec<String> {
    let mut patients: Vec<String> = Vec::new();
    for sample in samples {
        if !patients.contains(&sample.patient) {
            patients.push(sample.patient.clone());
        }
    }
    patients
}

/// The samples of one patient, as `(row, label)`.
///
/// The row is the index into the full list, so choosing from the second
/// drop-down still names a file. No patient means no samples: the second list
/// stays empty until the first is answered, rather than offering the whole
/// cohort under a heading that says otherwise.
fn samples_of<'a>(
    samples: &'a [crate::model::browser::SampleRow],
    patient: Option<&str>,
) -> Vec<(usize, &'a str)> {
    let Some(patient) = patient else {
        return Vec::new();
    };
    samples
        .iter()
        .enumerate()
        .filter(|(_, sample)| sample.patient == patient)
        .map(|(row, sample)| (row, sample.sample.as_deref().unwrap_or("—")))
        .collect()
}

/// Choose which sample is drawn.
///
/// Two drop-downs when the dataset has two levels — patient, then the samples
/// of that patient — and one when it has not. A single list of every
/// `patient — sample` pair is unreadable at the size a cohort actually is, and
/// the two-level shape is the one the whole toolchain already works in.
fn sample_picker(app: &mut MosnaApp, ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Sample")
            .color(theme::STEEL)
            .size(theme::size::HEADING)
            .strong(),
    );

    let mut chosen: Option<usize> = None;

    if has_two_levels(&app.network.samples) {
        let patients = patients_of(&app.network.samples);

        let caption = app
            .network
            .patient
            .clone()
            .unwrap_or_else(|| "— patient —".to_string());
        let mut picked_patient: Option<String> = None;
        list("network_patient", &caption, ui, |ui| {
            for patient in &patients {
                let on = app.network.patient.as_ref() == Some(patient);
                if ui.selectable_label(on, patient).clicked() {
                    picked_patient = Some(patient.clone());
                }
            }
        });
        if let Some(patient) = picked_patient {
            // A new patient invalidates the sample chosen under the old one.
            app.network.patient = Some(patient);
            app.network.selected = None;
        }

        ui.add_space(4.0);

        let caption = app
            .network
            .selected
            .and_then(|row| app.network.samples.get(row))
            .and_then(|sample| sample.sample.clone())
            .unwrap_or_else(|| "— sample —".to_string());
        let patient = app.network.patient.clone();
        let of_patient = samples_of(&app.network.samples, patient.as_deref());
        list("network_sample", &caption, ui, |ui| {
            if patient.is_none() {
                ui.label(
                    egui::RichText::new("Choose a patient first")
                        .color(theme::TEXT_MUTED)
                        .size(theme::size::SMALL),
                );
            }
            for (row, label) in &of_patient {
                if ui
                    .selectable_label(app.network.selected == Some(*row), *label)
                    .clicked()
                {
                    chosen = Some(*row);
                }
            }
        });
    } else {
        let caption = app
            .network
            .selected
            .and_then(|row| app.network.samples.get(row))
            .map(|sample| sample.patient.clone())
            .unwrap_or_else(|| "— patient —".to_string());
        list("network_sample", &caption, ui, |ui| {
            for (row, sample) in app.network.samples.iter().enumerate() {
                if ui
                    .selectable_label(app.network.selected == Some(row), &sample.patient)
                    .clicked()
                {
                    chosen = Some(row);
                }
            }
        });
    }

    if let Some(row) = chosen {
        let extension = app.browser.extension.clone();
        app.network.load(row, &extension);
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui
            .button("Refresh")
            .on_hover_text("Re-read the directory and start over")
            .clicked()
        {
            app.network.clear();
            discover(app);
        }
        if ui.button("Fit").on_hover_text("Frame the sample").clicked() {
            app.network.camera = None;
        }
    });

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Zoom")
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
        );
        // Buttons as well as the wheel: a trackpad, a tablet, or a hand that
        // would rather not scroll all deserve a way in.
        for (caption, factor) in [("−", 1.0 / ZOOM_PER_BUTTON), ("+", ZOOM_PER_BUTTON)] {
            if ui.button(caption).clicked() {
                if let Some(camera) = &mut app.network.camera {
                    let centre = app.network.canvas_centre;
                    camera.zoom_at(factor, centre);
                }
            }
        }
    });

    ui.add_space(4.0);
    if ui
        .checkbox(&mut app.network.show_edges, "Show edges")
        .changed()
    {
        app.network.prepared = None;
    }

    if app.network.has_sample() {
        let cells = app.network.n_cells();
        ui.label(
            egui::RichText::new(format!("{cells} cells"))
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
        );

        // Say when the view is not showing all of them, and when the edges
        // have been left out. A reader counting dots is entitled to know.
        let drawn = app.network.drawn_cells();
        let mut notes = Vec::new();
        if drawn > 0 && drawn < cells {
            notes.push(format!(
                "{drawn} drawn, with the edges between them — zoom in for all of them"
            ));
        }
        for note in notes {
            ui.label(
                egui::RichText::new(note)
                    .color(theme::TEXT_MUTED)
                    .size(theme::size::SMALL)
                    .italics(),
            );
        }
    }
}

fn colour_picker(app: &mut MosnaApp, ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Colour by")
            .color(theme::STEEL)
            .size(theme::size::HEADING)
            .strong(),
    );
    ui.label(
        egui::RichText::new(format!(
            "Up to {MAX_LAYERS} columns, blended into one picture."
        ))
        .color(theme::TEXT_MUTED)
        .size(theme::size::SMALL),
    );

    let columns: Vec<String> = app.network.columns().to_vec();
    let chosen: Vec<String> = app
        .network
        .layers
        .iter()
        .map(|layer| layer.column.clone())
        .collect();

    let caption = match chosen.len() {
        0 => "— none —".to_string(),
        1 => chosen[0].clone(),
        n => format!("{n} columns"),
    };

    let mut toggled: Option<String> = None;
    wide_list("network_colour", &caption, ui, |ui| {
        let full = chosen.len() >= MAX_LAYERS;
        for column in &columns {
            let mut on = chosen.contains(column);
            // A column that is not chosen when four already are cannot be
            // ticked; the ones that *are* chosen stay live, so there is always
            // a way back out of a full set.
            ui.add_enabled_ui(on || !full, |ui| {
                if ui.checkbox(&mut on, column).changed() {
                    toggled = Some(column.clone());
                }
            });
        }
    });

    if let Some(column) = toggled {
        app.network.toggle_layer(&column);
    }
}

fn tooltip_picker(app: &mut MosnaApp, ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("Show on hover")
            .color(theme::STEEL)
            .size(theme::size::HEADING)
            .strong(),
    );
    ui.label(
        egui::RichText::new("The columns the tooltip prints for the cell under the pointer.")
            .color(theme::TEXT_MUTED)
            .size(theme::size::SMALL),
    );

    let columns: Vec<String> = app.network.columns().to_vec();
    let mut toggled: Option<String> = None;

    let caption = match app.network.inspected.len() {
        0 => "— none —".to_string(),
        1 => app.network.inspected[0].clone(),
        n => format!("{n} columns"),
    };
    list("network_tooltip", &caption, ui, |ui| {
        for column in &columns {
            let mut on = app.network.inspected.contains(column);
            if ui.checkbox(&mut on, column).changed() {
                toggled = Some(column.clone());
            }
        }
    });

    if let Some(column) = toggled {
        match app.network.inspected.iter().position(|c| *c == column) {
            Some(index) => {
                app.network.inspected.remove(index);
            }
            None => app.network.inspected.push(column),
        }
        app.network.refresh_inspected();
    }
}

fn legend(app: &mut MosnaApp, ui: &mut egui::Ui) {
    if app.network.layers.is_empty() {
        return;
    }

    // Said once, above the bars: each bar describes its column *alone*, and
    // with more than one column no bar describes what is actually on screen.
    // The tooltip is where a blended cell is read.
    if app.network.layers.len() > 1 {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "Each cell blends the columns below, weighted by how strongly it \
                 expresses each. Hover a cell for its values.",
            )
            .color(theme::TEXT_MUTED)
            .size(theme::size::SMALL)
            .italics(),
        );
    }

    let mut repalette: Option<(usize, Palette)> = None;
    for index in 0..app.network.layers.len() {
        let layer = &app.network.layers[index];
        let column = layer.column.clone();
        let palette = layer.palette;
        let legend = layer.attribute().map(|attribute| attribute.legend(palette));

        ui.add_space(index.min(1) as f32 * 4.0);
        ui.label(
            egui::RichText::new(&column)
                .color(theme::ACCENT)
                .size(theme::size::HEADING)
                .strong(),
        );

        match legend {
            Some(Legend::Categories(entries)) => {
                let scroll = entries.len() > LEGEND_ROWS_BEFORE_SCROLL;
                let area = egui::ScrollArea::vertical()
                    .id_salt(("network_legend", index))
                    .max_height(if scroll { 260.0 } else { f32::INFINITY });
                area.show(ui, |ui| {
                    for (label, colour) in entries {
                        ui.horizontal(|ui| {
                            swatch(ui, to_colour(colour));
                            ui.label(
                                egui::RichText::new(label)
                                    .color(theme::TEXT)
                                    .size(theme::size::LABEL),
                            );
                        });
                    }
                });
            }
            Some(Legend::Colorbar { ramp, min, max }) => {
                // The picker sits on the bar it changes, so choosing a ramp is
                // choosing between things that are on screen rather than
                // between four names.
                if let Some(picked) = palette_picker(ui, index, palette) {
                    repalette = Some((index, picked));
                }
                colour_bar(ui, &ramp, min, max);
            }
            None => {
                ui.label(
                    egui::RichText::new("not in this sample")
                        .color(theme::TEXT_MUTED)
                        .size(theme::size::SMALL)
                        .italics(),
                );
            }
        }
        ui.add_space(6.0);
    }

    if let Some((index, palette)) = repalette {
        app.network.set_palette(index, palette);
    }
}

/// The ramp a measured column is drawn with.
///
/// Offered only for a measurement: a column of labels takes its colours from
/// the cluster palette the rest of the toolchain uses, and a picker that did
/// nothing would be worse than no picker.
fn palette_picker(ui: &mut egui::Ui, index: usize, current: Palette) -> Option<Palette> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Palette")
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
        );
        egui::ComboBox::from_id_salt(("network_palette", index))
            .selected_text(current.label())
            .width(ui.available_width().min(120.0))
            .wrap_mode(egui::TextWrapMode::Truncate)
            .show_ui(ui, |ui| {
                for palette in Palette::ALL {
                    if ui
                        .selectable_label(palette == current, palette.label())
                        .clicked()
                    {
                        picked = Some(palette);
                    }
                }
            });
    });
    picked
}

/// A square of colour, the size of a line of text.
fn swatch(ui: &mut egui::Ui, colour: Color32) {
    let side = theme::size::LABEL;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
    ui.painter().rect_filled(rect, 2.0, colour);
    ui.painter().rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
}

/// The continuous ramp, with the range it spans.
fn colour_bar(ui: &mut egui::Ui, ramp: &[Color32Source], min: f64, max: f64) {
    let width = ui.available_width().min(220.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::hover());

    // One rectangle per sample of the ramp, laid end to end. A step wider than
    // a pixel is invisible at this size, and this needs no texture.
    let step = rect.width() / ramp.len() as f32;
    for (index, colour) in ramp.iter().enumerate() {
        let left = rect.left() + index as f32 * step;
        let slice = Rect::from_min_max(
            Pos2::new(left, rect.top()),
            // A hair of overlap, so no seam of background shows between steps.
            Pos2::new(left + step + 1.0, rect.bottom()),
        );
        ui.painter().rect_filled(slice, 0.0, to_colour(*colour));
    }
    ui.painter().rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{min:.4}"))
                .color(theme::TEXT_MUTED)
                .size(theme::size::SMALL),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{max:.4}"))
                    .color(theme::TEXT_MUTED)
                    .size(theme::size::SMALL),
            );
        });
    });
}

/// The colour type the model speaks, kept separate from egui's.
type Color32Source = mosna_core::colormap::Rgb;

fn to_colour(rgb: Color32Source) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

// ---------------------------------------------------------------------------
// The canvas
// ---------------------------------------------------------------------------

fn canvas(app: &mut MosnaApp, ui: &mut egui::Ui) {
    // Everything that is left, to the pixel: the network is what the tab is
    // for, and any space kept back here is space taken from it.
    let size = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    // The same surface as a group box, so the canvas reads as one more panel
    // of the interface rather than as a hole cut in it.
    ui.painter().rect_filled(rect, 4.0, theme::SURFACE);
    app.network.canvas_centre = [rect.width() * 0.5, rect.height() * 0.5];
    app.network.canvas_size = [rect.width(), rect.height()];

    let Some(sample) = &app.network.sample else {
        return;
    };

    // The camera lives in canvas-local coordinates, so it survives the panel
    // being resized or moved.
    let camera = app
        .network
        .camera
        .get_or_insert_with(|| Camera::fit(sample.bounds, [rect.width(), rect.height()]));

    if response.dragged() {
        let delta = response.drag_delta();
        camera.pan([delta.x, delta.y]);
    }

    if response.hovered() {
        let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            if let Some(pointer) = response.hover_pos() {
                let local = pointer - rect.min;
                camera.zoom_at(ZOOM_PER_SCROLL_LINE.powf(scroll / 50.0), [local.x, local.y]);
            }
        }
    }

    let camera = *camera;
    let visible = camera.visible([rect.width(), rect.height()]);
    // The edges are drawn whenever the sample has any and the user wants them.
    // What keeps that affordable at two hundred thousand cells is not hiding
    // them but the subsample: an edge is kept only when both its ends are, so
    // the edge count falls with the cell count instead of independently.
    let with_edges = app.network.show_edges && !sample.edges.is_empty();

    prepare_if_stale(app, visible, with_edges);
    paint(app, ui, rect, camera);
    hover(app, ui, rect, camera, &response);
}

/// Rebuild the prepared geometry when what is on screen has left it.
fn prepare_if_stale(app: &mut MosnaApp, visible: Bounds, with_edges: bool) {
    let colouring: Vec<(String, Palette)> = app
        .network
        .layers
        .iter()
        .map(|layer| (layer.column.clone(), layer.palette))
        .collect();

    let stale = match &app.network.prepared {
        None => true,
        Some(prepared) => {
            !covers(prepared.region, visible)
                || prepared.with_edges != with_edges
                || prepared.colouring != colouring
        }
    };
    if !stale {
        return;
    }

    let Some(sample) = &app.network.sample else {
        return;
    };
    let region = mesh_region(visible);
    let mut rows = sample.index.in_view(region);
    rows.sort_unstable();

    // Past the budget, every stride-th cell. Sorted first, so the subsample is
    // spread over the region rather than over whichever buckets came first.
    let stride = stride_for(rows.len());
    let rows: Vec<u32> = rows.into_iter().step_by(stride).collect();

    let cells: Vec<Point> = rows
        .iter()
        .map(|&row| flip(sample.positions[row as usize]))
        .collect();

    // One colour map per chosen column, built once for the whole pass rather
    // than once per cell: the panel asks for up to sixty thousand colours in
    // one go, and up to four times over.
    let channels: Vec<(crate::model::network::Colouring<'_>, &Attribute)> = app
        .network
        .layers
        .iter()
        .filter_map(|layer| {
            let attribute = layer.attribute()?;
            Some((attribute.colouring(layer.palette), attribute))
        })
        .collect();

    let colours: Vec<Color32> = if channels.is_empty() {
        vec![theme::STEEL; rows.len()]
    } else {
        rows.iter()
            .map(|&row| {
                let row = row as usize;
                // A fixed array, not a `Vec`: this runs once per cell, and
                // there are at most `MAX_LAYERS` of them.
                let mut blend: [Channel; MAX_LAYERS] = [None; MAX_LAYERS];
                for (slot, (colouring, attribute)) in blend.iter_mut().zip(&channels) {
                    *slot = attribute
                        .weight(row)
                        .map(|weight| (colouring.colour(row), weight));
                }
                to_colour(fuse(&blend))
            })
            .collect()
    };

    let edges = if with_edges {
        let mut kept: Vec<bool> = vec![false; sample.n_cells()];
        for &row in &rows {
            kept[row as usize] = true;
        }
        // Both ends, not either: when nothing was thinned this is the same
        // set apart from the edges leaving the region, and when the subsample
        // is biting it is what keeps the edges from outnumbering the cells
        // they connect several times over.
        sample
            .edges
            .iter()
            .filter(|(a, b)| kept[*a as usize] && kept[*b as usize])
            .map(|(a, b)| {
                (
                    flip(sample.positions[*a as usize]),
                    flip(sample.positions[*b as usize]),
                )
            })
            .collect()
    } else {
        Vec::new()
    };

    app.network.prepared = Some(Prepared {
        cells,
        colours,
        edges,
        region,
        colouring,
        with_edges,
    });
}

/// Draw the prepared geometry through the camera.
fn paint(app: &MosnaApp, ui: &mut egui::Ui, rect: Rect, camera: Camera) {
    let Some(prepared) = &app.network.prepared else {
        return;
    };
    let Some(sample) = &app.network.sample else {
        return;
    };

    let painter = ui.painter_at(rect);
    let to_screen = |point: Point| {
        Pos2::new(
            point[0] * camera.scale + camera.offset[0] + rect.left(),
            point[1] * camera.scale + camera.offset[1] + rect.top(),
        )
    };

    // The edges first, so a cell is never hidden behind one.
    if !prepared.edges.is_empty() {
        let mut mesh = Mesh::default();
        let colour = theme::BORDER;
        for (a, b) in &prepared.edges {
            add_segment(&mut mesh, to_screen(*a), to_screen(*b), 1.0, colour);
        }
        painter.add(mesh);
    }

    let radius = point_radius(camera.scale, sample.index.spacing());
    // The ring of offsets is the same for every cell, so it is computed once
    // and translated, rather than a sine and a cosine per cell per side.
    let ring = ring_offsets(radius);
    let mut mesh = Mesh::default();
    for (index, centre) in prepared.cells.iter().enumerate() {
        let colour = prepared
            .colours
            .get(index)
            .copied()
            .unwrap_or(theme::STEEL);
        add_disc(&mut mesh, to_screen(*centre), &ring, colour);
    }
    painter.add(mesh);
}

/// Add a quad covering the segment from `a` to `b`.
fn add_segment(mesh: &mut Mesh, a: Pos2, b: Pos2, width: f32, colour: Color32) {
    let along = b - a;
    let length = along.length();
    if length <= f32::EPSILON {
        return;
    }
    // The normal of the segment, at half the width on each side.
    let normal = Vec2::new(-along.y, along.x) / length * (width * 0.5);
    let base = mesh.vertices.len() as u32;
    for corner in [a + normal, a - normal, b - normal, b + normal] {
        mesh.colored_vertex(corner, colour);
    }
    mesh.add_triangle(base, base + 1, base + 2);
    mesh.add_triangle(base, base + 2, base + 3);
}

/// The offsets of a disc's rim from its centre, at this radius.
fn ring_offsets(radius: f32) -> Vec<Vec2> {
    let sides = circle_segments(radius);
    (0..sides)
        .map(|side| {
            let angle = std::f32::consts::TAU * side as f32 / sides as f32;
            Vec2::new(radius * angle.cos(), radius * angle.sin())
        })
        .collect()
}

/// Add a disc centred on `centre`, from a rim computed once by
/// [`ring_offsets`].
///
/// A fan around the centre: one vertex in the middle, one per side, and a
/// triangle for each gap between them.
fn add_disc(mesh: &mut Mesh, centre: Pos2, ring: &[Vec2], colour: Color32) {
    let base = mesh.vertices.len() as u32;
    mesh.colored_vertex(centre, colour);
    for offset in ring {
        mesh.colored_vertex(centre + *offset, colour);
    }
    let sides = ring.len() as u32;
    for side in 0..sides {
        // The last triangle closes the ring back onto the first rim vertex.
        let next = (side + 1) % sides;
        mesh.add_triangle(base, base + 1 + side, base + 1 + next);
    }
}

/// Name the cell under the pointer, and say what it carries.
///
/// Every coloured column, not only one: the cell's colour is a blend of all of
/// them and cannot be read back into values, so the tooltip is where the
/// values are. That is the trade the single picture makes.
fn hover(app: &MosnaApp, ui: &mut egui::Ui, rect: Rect, camera: Camera, response: &egui::Response) {
    let Some(pointer) = response.hover_pos() else {
        return;
    };
    let Some(sample) = &app.network.sample else {
        return;
    };

    // Every cell answers the pointer, including one the subsample left out of
    // the mesh. At a zoom where the subsample bites, the cells overlap several
    // deep and something is painted under the pointer either way; naming the
    // cell that is really there beats naming the one that happened to be kept.
    let local = pointer - rect.min;
    let at = camera.screen_to_world([local.x, local.y]);
    // The search radius in world units is the drawn radius back through the
    // zoom, so the target stays the size it looks.
    let radius = point_radius(camera.scale, sample.index.spacing()) * HOVER_SLACK / camera.scale;

    let Some(row) = sample.index.nearest(&sample.positions, at, radius) else {
        return;
    };
    let row = row as usize;

    let centre = camera.world_to_screen(sample.positions[row]);
    let centre = Pos2::new(centre[0] + rect.left(), centre[1] + rect.top());
    ui.painter_at(rect).circle_stroke(
        centre,
        point_radius(camera.scale, sample.index.spacing()) + 3.0,
        Stroke::new(1.5, theme::ACCENT_STRONG),
    );

    egui::Tooltip::for_enabled(response)
        .at_pointer()
        .show(|ui| {
            ui.label(
                egui::RichText::new(format!("cell {row}"))
                    .color(theme::TEXT_MUTED)
                    .size(theme::size::SMALL),
            );
            let position = sample.positions[row];
            ui.label(
                egui::RichText::new(format!("({:.2}, {:.2})", position[0], position[1]))
                    .color(theme::TEXT_MUTED)
                    .size(theme::size::SMALL),
            );

            let coloured: Vec<(&str, String)> = app
                .network
                .layers
                .iter()
                .filter_map(|layer| {
                    layer
                        .attribute()
                        .map(|attribute| (layer.column.as_str(), attribute.text(row)))
                })
                .collect();
            if !coloured.is_empty() {
                ui.separator();
                for (column, value) in coloured {
                    entry(ui, column, &value);
                }
            }

            if !app.network.inspected.is_empty() {
                ui.separator();
                for (column, values) in app
                    .network
                    .inspected
                    .iter()
                    .zip(&app.network.inspected_values)
                {
                    entry(
                        ui,
                        column,
                        values.get(row).map(String::as_str).unwrap_or("—"),
                    );
                }
            }
        });
}

/// One `name: value` line of the tooltip.
fn entry(ui: &mut egui::Ui, name: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{name}:"))
                .color(theme::TEXT_MUTED)
                .size(theme::size::LABEL),
        );
        ui.label(
            egui::RichText::new(value)
                .color(theme::TEXT)
                .size(theme::size::LABEL)
                .strong(),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::browser::SampleRow;
    use mosna_io::Table;

    /// A sample on disk, and the row that points at it.
    fn a_sample(directory: &std::path::Path, with_edges: bool) -> SampleRow {
        let nodes = Table::from_columns(vec![
            ("X".into(), Table::f64_array([0.0, 1.0, 2.0, 3.0])),
            ("Y".into(), Table::f64_array([0.0, 1.0, 0.0, 1.0])),
            (
                "phenotype".into(),
                Table::string_array(["cancer", "immune", "cancer", "stroma"]),
            ),
            ("CD8".into(), Table::f64_array([0.25, 0.75, 1.5, 2.25])),
        ])
        .unwrap();
        let nodes_path = directory.join("nodes_patient-1_sample-1.parquet");
        mosna_io::write::write_parquet::write_parquet(&nodes, &nodes_path).unwrap();

        let edges_file = with_edges.then(|| {
            let edges = Table::from_edges(&[(0, 1), (1, 2), (2, 3)]).unwrap();
            let name = "edges_patient-1_sample-1.parquet";
            mosna_io::write::write_parquet::write_parquet(&edges, directory.join(name)).unwrap();
            name.to_string()
        });

        SampleRow {
            patient: "1".into(),
            sample: Some("1".into()),
            nodes_file: "nodes_patient-1_sample-1.parquet".into(),
            edges_file,
            nodes_path,
        }
    }

    fn state(directory: &std::path::Path, with_edges: bool) -> NetworkState {
        NetworkState {
            samples: vec![a_sample(directory, with_edges)],
            x_column: "X".into(),
            y_column: "Y".into(),
            ..Default::default()
        }
    }

    #[test]
    fn loading_a_sample_offers_its_columns() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.load(0, "parquet");

        assert!(network.error.is_none(), "{:?}", network.error);
        assert!(network.has_sample());
        assert_eq!(network.n_cells(), 4);
        assert!(network.columns().contains(&"CD8".to_string()));
        assert_eq!(network.selected, Some(0));
    }

    /// Step 1 may not have run. The cells are still worth drawing.
    #[test]
    fn a_sample_whose_edges_are_missing_still_loads() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), false);
        network.load(0, "parquet");

        assert!(network.has_sample());
        assert!(network.error.is_none());
    }

    #[test]
    fn choosing_a_label_column_builds_a_list_legend() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.load(0, "parquet");

        network.toggle_layer("phenotype");

        let layer = &network.layers[0];
        match layer.attribute().expect("the column was read").legend(layer.palette) {
            Legend::Categories(entries) => {
                assert_eq!(entries.len(), 3, "cancer, immune, stroma");
            }
            other => panic!("expected a list, got {other:?}"),
        }
    }

    #[test]
    fn choosing_a_measured_column_builds_a_colour_bar() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.load(0, "parquet");

        network.toggle_layer("CD8");

        let layer = &network.layers[0];
        match layer.attribute().unwrap().legend(layer.palette) {
            Legend::Colorbar { min, max, .. } => assert_eq!((min, max), (0.25, 2.25)),
            other => panic!("expected a colour bar, got {other:?}"),
        }
    }

    /// Four channels, and no more: the fifth click is refused rather than
    /// silently replacing one of the four.
    #[test]
    fn no_more_than_four_columns_can_be_chosen() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.load(0, "parquet");

        for column in ["phenotype", "CD8", "niches", "x", "y"] {
            network.toggle_layer(column);
        }
        assert_eq!(network.layers().len(), MAX_LAYERS);
        assert!(
            !network.layers().iter().any(|layer| layer.column == "y"),
            "the fifth column got in"
        );
    }

    /// And each of them opens on a ramp of its own: two channels on the same
    /// ramp blend into a colour that says nothing about which is which.
    #[test]
    fn each_channel_opens_on_a_palette_no_other_channel_has() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.load(0, "parquet");

        for column in ["phenotype", "CD8", "niches", "x"] {
            network.toggle_layer(column);
        }
        let palettes: Vec<Palette> = network.layers().iter().map(|l| l.palette).collect();
        for (index, palette) in palettes.iter().enumerate() {
            assert!(
                !palettes[index + 1..].contains(palette),
                "{} is used by two channels",
                palette.label()
            );
        }
    }

    /// A channel removed from the middle frees its colour for the next one, so
    /// a set of four never ends up with two channels on the same ramp.
    #[test]
    fn a_removed_channel_gives_its_palette_back() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.load(0, "parquet");

        for column in ["phenotype", "CD8", "niches"] {
            network.toggle_layer(column);
        }
        let freed = network.layers()[1].palette;
        network.toggle_layer("CD8");
        network.toggle_layer("x");

        assert_eq!(network.layers().len(), 3);
        assert_eq!(
            network.layers()[2].palette,
            freed,
            "the new channel did not take the freed palette"
        );
    }

    /// Clicking a column that is already a view removes it.
    #[test]
    fn choosing_the_same_column_twice_removes_its_channel() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.load(0, "parquet");

        network.toggle_layer("CD8");
        network.toggle_layer("CD8");
        assert!(network.layers().is_empty());
    }

    /// Changing the colouring must drop the prepared geometry, or the cells
    /// keep the colours of the column that is no longer selected.
    #[test]
    fn changing_the_colouring_invalidates_the_prepared_geometry() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.load(0, "parquet");

        network.prepared = Some(prepared());
        network.toggle_layer("phenotype");
        assert!(network.prepared.is_none(), "adding a view kept the geometry");
    }

    /// And so must changing a palette: the colours are resolved into the
    /// prepared geometry, so the picture does not change until it is dropped.
    #[test]
    fn changing_a_palette_invalidates_the_prepared_geometry() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.load(0, "parquet");
        network.toggle_layer("CD8");

        network.prepared = Some(prepared());
        network.set_palette(0, Palette::Greens);
        assert_eq!(network.layers()[0].palette, Palette::Greens);
        assert!(network.prepared.is_none());

        // The same palette again is not a change, and must not throw away the
        // geometry every frame the picker is looked at.
        network.prepared = Some(prepared());
        network.set_palette(0, Palette::Greens);
        assert!(network.prepared.is_some());
    }

    fn prepared() -> Prepared {
        Prepared {
            cells: Vec::new(),
            colours: Vec::new(),
            edges: Vec::new(),
            region: Bounds {
                min: [0.0, 0.0],
                max: [1.0, 1.0],
            },
            colouring: Vec::new(),
            with_edges: false,
        }
    }

    #[test]
    fn the_hover_columns_are_read_once_and_kept() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.load(0, "parquet");

        network.inspected = vec!["phenotype".into(), "CD8".into()];
        network.refresh_inspected();

        assert_eq!(network.inspected_values.len(), 2);
        assert_eq!(network.inspected_values[0][1], "immune");
        assert_eq!(network.inspected_values[1][3], "2.25");
    }

    #[test]
    fn a_column_that_is_not_there_is_reported_rather_than_panicking() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.load(0, "parquet");

        network.toggle_layer("CD99");

        assert!(network.layers()[0].attribute().is_none());
        assert!(network.error.as_deref().unwrap().contains("CD99"));
    }

    #[test]
    fn a_missing_coordinate_column_leaves_no_half_loaded_sample() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.x_column = "x_pos".into();
        network.load(0, "parquet");

        assert!(!network.has_sample());
        assert!(network.error.as_deref().unwrap().contains("x_pos"));
    }

    /// Refresh, and a change of working directory, must leave the tab exactly
    /// as the application opens it — not merely without a sample.
    #[test]
    fn clearing_forgets_the_sample_and_everything_about_it() {
        let directory = tempfile::tempdir().unwrap();
        let mut network = state(directory.path(), true);
        network.load(0, "parquet");
        network.toggle_layer("phenotype");
        network.inspected = vec!["CD8".into()];
        network.refresh_inspected();
        network.patient = Some("1".into());
        network.show_edges = false;
        network.camera = Some(Camera {
            scale: 3.0,
            offset: [1.0, 2.0],
        });

        network.clear();

        assert!(!network.has_sample());
        assert!(network.samples.is_empty());
        assert!(network.selected.is_none());
        assert!(network.patient.is_none());
        assert!(network.camera.is_none());
        assert!(network.layers().is_empty());
        assert!(network.inspected.is_empty());
        assert!(network.inspected_values.is_empty());
        assert!(network.prepared.is_none());
        assert!(network.error.is_none());
        assert!(
            network.show_edges,
            "the edges are on when the application opens, so they are on again"
        );
    }

    // -----------------------------------------------------------------------
    // The two drop-downs
    // -----------------------------------------------------------------------

    fn row(patient: &str, sample: Option<&str>) -> SampleRow {
        SampleRow {
            patient: patient.into(),
            sample: sample.map(str::to_string),
            nodes_file: String::new(),
            edges_file: None,
            nodes_path: std::path::PathBuf::new(),
        }
    }

    #[test]
    fn a_two_level_dataset_is_picked_by_patient_then_by_sample() {
        let samples = [
            row("P1", Some("a")),
            row("P1", Some("b")),
            row("P2", Some("a")),
        ];

        assert!(has_two_levels(&samples));
        assert_eq!(patients_of(&samples), vec!["P1", "P2"]);

        let of_p1 = samples_of(&samples, Some("P1"));
        assert_eq!(of_p1, vec![(0, "a"), (1, "b")]);
        assert_eq!(samples_of(&samples, Some("P2")), vec![(2, "a")]);
    }

    /// The row must index the whole list, not the filtered one, or choosing
    /// the second patient's sample would load the first patient's file.
    #[test]
    fn a_sample_keeps_the_row_of_the_file_it_names() {
        let samples = [row("P1", Some("a")), row("P2", Some("a"))];
        let (row_of_p2, _) = samples_of(&samples, Some("P2"))[0];
        assert_eq!(row_of_p2, 1);
        assert_eq!(samples[row_of_p2].patient, "P2");
    }

    #[test]
    fn a_single_level_dataset_has_only_patients() {
        let samples = [row("01", None), row("02", None)];
        assert!(!has_two_levels(&samples));
        assert_eq!(patients_of(&samples), vec!["01", "02"]);
    }

    /// Until a patient is chosen there is nothing to choose from.
    #[test]
    fn no_patient_means_no_samples_to_choose() {
        let samples = [row("P1", Some("a")), row("P2", Some("b"))];
        assert!(samples_of(&samples, None).is_empty());
    }
}
