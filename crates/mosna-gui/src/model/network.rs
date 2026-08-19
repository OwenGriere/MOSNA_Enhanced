//! The interactive network view: its data, and the arithmetic behind it.
//!
//! Everything here is pure — loading a sample, deciding what a column means,
//! building the legend, finding the cell under the pointer, moving the camera,
//! choosing how much detail to draw. The panel that draws it holds no
//! arithmetic of its own, so all of this is testable without a window.
//!
//! # Why a model at all
//!
//! The static figures the pipeline writes answer "what does this sample look
//! like". They cannot answer "what is *that* cell", which is the question a
//! biologist actually has in front of a network. That question needs the
//! attributes to stay attached to the coordinates, and needs them at the
//! pointer — so the data has to live in the interface, not in a PNG.

use std::path::Path;

use mosna_core::colormap::{blues, greens, make_cluster_cmap, purples, reds, Gradient, Rgb};
use mosna_io::read::get_opener::{read_table, Extension};
use mosna_io::Table;

/// A cell's position, in the units of the nodes file.
///
/// `f32` rather than `f64`: these are screen coordinates in waiting, a slide is
/// tens of thousands of microns across, and two hundred thousand cells is two
/// hundred thousand of these.
pub type Point = [f32; 2];

/// Most distinct values a numeric column may have and still be read as labels
/// rather than as a measurement.
///
/// `niches` is written back as a float — `0.0`, `1.0`, `2.0` — and is a label
/// in every sense that matters. A biomarker intensity is not. The line between
/// them is whether the values are whole numbers and few, and this is "few".
pub const MAX_LEVELS: usize = 32;

// ---------------------------------------------------------------------------
// Extent
// ---------------------------------------------------------------------------

/// The rectangle a set of points occupies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: Point,
    pub max: Point,
}

impl Bounds {
    /// The bounding box of `points`, or `None` if there is nothing to bound.
    ///
    /// Non-finite coordinates are ignored rather than allowed to swallow the
    /// box: one `NaN` would otherwise make every bound `NaN` and the view
    /// would open on nothing.
    pub fn of(points: &[Point]) -> Option<Self> {
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        let mut seen = false;

        for point in points {
            if !point[0].is_finite() || !point[1].is_finite() {
                continue;
            }
            seen = true;
            for axis in 0..2 {
                min[axis] = min[axis].min(point[axis]);
                max[axis] = max[axis].max(point[axis]);
            }
        }

        seen.then_some(Self { min, max })
    }

    pub fn width(&self) -> f32 {
        self.max[0] - self.min[0]
    }

    pub fn height(&self) -> f32 {
        self.max[1] - self.min[1]
    }

    pub fn centre(&self) -> Point {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
        ]
    }

    /// Grow by `margin` on every side.
    pub fn expanded(&self, margin: f32) -> Self {
        Self {
            min: [self.min[0] - margin, self.min[1] - margin],
            max: [self.max[0] + margin, self.max[1] + margin],
        }
    }
}

// ---------------------------------------------------------------------------
// What a column means
// ---------------------------------------------------------------------------

/// A ramp a measured column can be drawn with.
///
/// Four, because the tab draws up to [`MAX_LAYERS`] columns side by side and
/// two views on the same ramp are two pictures nobody can tell apart. Which
/// one a view uses is the user's to change: the default is only an opening
/// position, and the right hue for a marker is whatever the person reading it
/// has been looking at for a year.
///
/// A column of *labels* ignores all of this — its colours come from the
/// cluster palette the figures use, so a niche keeps the colour it has in
/// every other output of the toolchain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Palette {
    #[default]
    Reds,
    Blues,
    Greens,
    Purples,
}

impl Palette {
    /// The four, in the order they are offered and handed out.
    pub const ALL: [Palette; 4] = [
        Palette::Reds,
        Palette::Blues,
        Palette::Greens,
        Palette::Purples,
    ];

    /// What the picker calls it.
    pub fn label(self) -> &'static str {
        match self {
            Palette::Reds => "Reds",
            Palette::Blues => "Blues",
            Palette::Greens => "Greens",
            Palette::Purples => "Purples",
        }
    }

    /// The ramp itself.
    pub fn gradient(self) -> Gradient {
        match self {
            Palette::Reds => reds(),
            Palette::Blues => blues(),
            Palette::Greens => greens(),
            Palette::Purples => purples(),
        }
    }

    /// The palette the `index`-th view opens with.
    ///
    /// Distinct for as long as there are palettes, which — since a view cannot
    /// be added past the fourth — means always. Adding a column has to give it
    /// a colour of its own without asking, or the second view arrives looking
    /// exactly like the first.
    pub fn nth(index: usize) -> Palette {
        Palette::ALL[index % Palette::ALL.len()]
    }

    /// A swatch of the ramp, for the picker and the legend.
    pub fn ramp(self) -> Vec<Rgb> {
        let gradient = self.gradient();
        (0..RAMP_STEPS)
            .map(|step| gradient.sample(step as f64 / (RAMP_STEPS - 1) as f64))
            .collect()
    }
}

/// A column of the nodes file, read as something that can colour a cell.
#[derive(Debug, Clone, PartialEq)]
pub enum Attribute {
    /// A column of labels: phenotypes, niches, anything with a vocabulary.
    Categorical {
        /// The distinct values, in first-seen order.
        levels: Vec<String>,
        /// The level of each cell, indexing `levels`.
        codes: Vec<u32>,
    },
    /// A measurement: a biomarker intensity, an area, a score.
    Continuous {
        values: Vec<f64>,
        /// The finite range, used by the colour bar. Equal when every value is
        /// the same, which the colouring has to survive.
        min: f64,
        max: f64,
    },
}

impl Attribute {
    /// Read `column`, deciding for itself whether it is labels or a
    /// measurement.
    ///
    /// Two questions, answered by two different things. Whether the column
    /// holds numbers at all is the storage type's to answer. Whether those
    /// numbers are a measurement is the values' — the storage type cannot
    /// help there, since `niches` and a CD8 intensity are both `double` in
    /// the parquet file.
    pub fn read(table: &Table, column: &str) -> anyhow::Result<Self> {
        if !table.is_numeric_column(column)? {
            return Ok(Self::labels(table.opt_string_column(column)?));
        }

        let values = table.f64_column(column)?;
        if reads_as_labels(&values) {
            return Ok(Self::numeric_labels(&values, table.string_column(column)?));
        }

        let (min, max) = finite_range(&values);
        Ok(Self::Continuous { values, min, max })
    }

    /// How many cells the attribute covers.
    pub fn len(&self) -> usize {
        match self {
            Attribute::Categorical { codes, .. } => codes.len(),
            Attribute::Continuous { values, .. } => values.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The colour of one cell.
    ///
    /// For a handful of cells — a legend swatch, a test. Colouring a whole
    /// sample goes through [`Self::colouring`], which builds the map once
    /// instead of once per cell.
    pub fn colour(&self, row: usize, palette: Palette) -> Rgb {
        self.colouring(palette).colour(row)
    }

    /// The colour map, resolved once, ready to colour many cells.
    ///
    /// Both maps are built rather than looked up — `make_cluster_cmap`
    /// allocates a palette, `reds` a list of stops — and the panel asks for
    /// up to sixty thousand colours in one pass. Building the map inside that
    /// loop was sixty thousand allocations per rebuild, for one answer that
    /// never changes.
    pub fn colouring(&self, palette: Palette) -> Colouring<'_> {
        match self {
            Attribute::Categorical { levels, .. } => Colouring {
                attribute: self,
                levels: make_cluster_cmap(levels.len()),
                gradient: None,
            },
            Attribute::Continuous { .. } => Colouring {
                attribute: self,
                levels: Vec::new(),
                gradient: Some(palette.gradient()),
            },
        }
    }

    /// How strongly a cell expresses this column, in `[0, 1]`, or `None` when
    /// it has no value for that cell.
    ///
    /// This is what [`fuse`] weighs the columns by. A measurement weighs where
    /// its value sits in its own range — so a blend is a blend of what is
    /// *high* here, not of what happens to be measured in large numbers, and a
    /// column in counts does not shout down a column in fractions.
    ///
    /// A label weighs one: a phenotype is a fact about a cell, not a degree of
    /// one, and a cell either has it or has nothing there.
    pub fn weight(&self, row: usize) -> Option<f32> {
        match self {
            Attribute::Categorical { levels, codes } => codes
                .get(row)
                .filter(|code| (**code as usize) < levels.len())
                .map(|_| 1.0),
            Attribute::Continuous { values, min, max } => values
                .get(row)
                .filter(|value| value.is_finite())
                .map(|value| normalise(*value, *min, *max) as f32),
        }
    }

    /// What the legend should show.
    pub fn legend(&self, palette: Palette) -> Legend {
        match self {
            Attribute::Categorical { levels, .. } => {
                let palette = make_cluster_cmap(levels.len());
                Legend::Categories(
                    levels
                        .iter()
                        .cloned()
                        .zip(palette.into_iter().chain(std::iter::repeat(Gradient::BAD)))
                        .collect(),
                )
            }
            Attribute::Continuous { min, max, .. } => Legend::Colorbar {
                ramp: palette.ramp(),
                min: *min,
                max: *max,
            },
        }
    }

    /// The value of one cell, as the tooltip should print it.
    pub fn text(&self, row: usize) -> String {
        match self {
            Attribute::Categorical { levels, codes } => codes
                .get(row)
                .and_then(|code| levels.get(*code as usize))
                .cloned()
                .unwrap_or_else(|| MISSING.to_string()),
            Attribute::Continuous { values, .. } => match values.get(row) {
                // Four significant figures: an intensity is not measured to
                // seventeen, and a tooltip that wraps is a tooltip nobody reads.
                Some(value) if value.is_finite() => format!("{value:.4}"),
                _ => MISSING.to_string(),
            },
        }
    }

    /// Build a vocabulary from text, in first-seen order.
    ///
    /// First-seen and not sorted: it is the order `generate_cmap` uses on the
    /// Python side, so the same phenotype keeps the same colour between the
    /// interactive view and the figure beside it.
    fn labels(text: Vec<Option<String>>) -> Self {
        let mut levels: Vec<String> = Vec::new();
        let mut codes = Vec::with_capacity(text.len());

        for value in text {
            match value {
                Some(label) => {
                    let code = match levels.iter().position(|seen| *seen == label) {
                        Some(index) => index,
                        None => {
                            levels.push(label);
                            levels.len() - 1
                        }
                    };
                    codes.push(code as u32);
                }
                // Out of range of `levels`, which `colour` and `text` read as
                // "no value" — a missing label is not a level of its own.
                None => codes.push(u32::MAX),
            }
        }

        Self::Categorical { levels, codes }
    }

    /// A vocabulary from a numeric column — `niches`, an integer cluster id.
    ///
    /// The levels are found among the *numbers* and only then named from the
    /// rendered text, because a `NaN` renders as the string `NaN` and would
    /// otherwise become a level of its own: a niche called "not a number",
    /// with a colour and a line in the legend.
    fn numeric_labels(values: &[f64], text: Vec<String>) -> Self {
        let mut levels: Vec<String> = Vec::new();
        let mut seen: Vec<f64> = Vec::new();
        let mut codes = Vec::with_capacity(values.len());

        for (row, value) in values.iter().enumerate() {
            if !value.is_finite() {
                codes.push(u32::MAX);
                continue;
            }
            let code = match seen.iter().position(|level| level == value) {
                Some(index) => index,
                None => {
                    seen.push(*value);
                    levels.push(text.get(row).cloned().unwrap_or_else(|| value.to_string()));
                    levels.len() - 1
                }
            };
            codes.push(code as u32);
        }

        Self::Categorical { levels, codes }
    }
}

/// An attribute's colour map, built once and asked many times.
pub struct Colouring<'a> {
    attribute: &'a Attribute,
    /// One colour per level, for a vocabulary.
    levels: Vec<Rgb>,
    /// The ramp, for a measurement.
    gradient: Option<Gradient>,
}

impl Colouring<'_> {
    /// The colour of one cell.
    ///
    /// A cell with no value — a null, or a `NaN` in a measured column — takes
    /// the colour map's "bad" grey, the same one the figures use for a missing
    /// cell, rather than the colour of zero.
    pub fn colour(&self, row: usize) -> Rgb {
        match self.attribute {
            Attribute::Categorical { codes, .. } => codes
                .get(row)
                .and_then(|code| self.levels.get(*code as usize))
                .copied()
                .unwrap_or(Gradient::BAD),
            Attribute::Continuous { values, min, max } => match (values.get(row), &self.gradient) {
                (Some(value), Some(gradient)) if value.is_finite() => {
                    gradient.sample(normalise(*value, *min, *max))
                }
                _ => Gradient::BAD,
            },
        }
    }
}

/// What the tooltip prints for a cell that has no value.
const MISSING: &str = "—";

/// Whether a column of numbers is really a column of labels.
fn reads_as_labels(values: &[f64]) -> bool {
    let mut levels: Vec<f64> = Vec::new();
    for value in values {
        if !value.is_finite() {
            continue;
        }
        if value.fract() != 0.0 {
            return false;
        }
        if !levels.contains(value) {
            levels.push(*value);
            if levels.len() > MAX_LEVELS {
                return false;
            }
        }
    }
    !levels.is_empty()
}

/// The range of the values that are numbers.
///
/// A column that is entirely missing has no range; `(0, 1)` keeps the colour
/// bar drawable and every cell grey, which is the truth about it.
fn finite_range(values: &[f64]) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for value in values.iter().filter(|v| v.is_finite()) {
        min = min.min(*value);
        max = max.max(*value);
    }
    if min <= max {
        (min, max)
    } else {
        (0.0, 1.0)
    }
}

/// Where `value` sits in `[min, max]`, as `0..=1`.
///
/// A column with a single value has no spread to place anything in; it takes
/// the bottom of the ramp rather than dividing by zero.
fn normalise(value: f64, min: f64, max: f64) -> f64 {
    if max > min {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// What the panel draws beside the network to explain its colours.
#[derive(Debug, Clone, PartialEq)]
pub enum Legend {
    /// One swatch per value.
    Categories(Vec<(String, Rgb)>),
    /// A continuous ramp between two labelled ends.
    Colorbar {
        /// The ramp, sampled from low to high.
        ramp: Vec<Rgb>,
        min: f64,
        max: f64,
    },
}

/// How many samples of the ramp a colour bar carries.
pub const RAMP_STEPS: usize = 64;

// ---------------------------------------------------------------------------
// Finding a cell
// ---------------------------------------------------------------------------

/// A uniform grid over the cells, for answering "what is under the pointer".
///
/// A linear scan is a quarter of a million distance computations per frame at
/// the sizes this view is for. The grid makes it a handful, and it is built
/// once per sample.
#[derive(Debug, Clone)]
pub struct SpatialIndex {
    origin: Point,
    /// Side of one bucket, in world units.
    cell: f32,
    columns: usize,
    rows: usize,
    buckets: Vec<Vec<u32>>,
}

impl SpatialIndex {
    /// Index `points`, sized so a bucket holds a couple of cells.
    pub fn build(points: &[Point], bounds: Bounds) -> Self {
        // One bucket per cell on average: the side of the square each cell
        // gets if they were spread evenly. That is also, near enough, the
        // distance between neighbours, which is why `spacing` can return it.
        let area = (bounds.width() as f64 * bounds.height() as f64).max(f64::MIN_POSITIVE);
        let cell = (area / points.len().max(1) as f64).sqrt() as f32;
        // A degenerate sample — every cell at the same place, or a single one —
        // gives a zero or non-finite side, which would divide by zero below.
        let cell = if cell.is_finite() && cell > f32::EPSILON {
            cell
        } else {
            1.0
        };

        let columns = ((bounds.width() / cell).ceil() as usize + 1).max(1);
        let rows = ((bounds.height() / cell).ceil() as usize + 1).max(1);

        let mut index = Self {
            origin: bounds.min,
            cell,
            columns,
            rows,
            buckets: vec![Vec::new(); columns * rows],
        };
        for (row, point) in points.iter().enumerate() {
            if let Some(bucket) = index.bucket_of(*point) {
                index.buckets[bucket].push(row as u32);
            }
        }
        index
    }

    /// The bucket a point falls in, or `None` if it falls outside the grid —
    /// which a non-finite coordinate does, and which is where it belongs.
    fn bucket_of(&self, point: Point) -> Option<usize> {
        let (column, row) = self.cell_of(point)?;
        Some(row * self.columns + column)
    }

    /// The column and row a point falls in, clamped to nothing: a point
    /// outside the grid has no cell.
    fn cell_of(&self, point: Point) -> Option<(usize, usize)> {
        let column = ((point[0] - self.origin[0]) / self.cell).floor();
        let row = ((point[1] - self.origin[1]) / self.cell).floor();
        if !column.is_finite() || !row.is_finite() || column < 0.0 || row < 0.0 {
            return None;
        }
        let (column, row) = (column as usize, row as usize);
        (column < self.columns && row < self.rows).then_some((column, row))
    }

    /// The range of columns and rows a rectangle touches, clamped to the grid.
    fn span(&self, view: Bounds) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let axis = |low: f32, high: f32, origin: f32, count: usize| {
            let first = ((low - origin) / self.cell).floor();
            let last = ((high - origin) / self.cell).floor();
            let first = first.max(0.0).min(count as f32) as usize;
            // Inclusive of the last bucket the rectangle touches, hence `+ 1`.
            let last = (last.max(0.0) as usize + 1).min(count);
            first..last.max(first)
        };
        (
            axis(view.min[0], view.max[0], self.origin[0], self.columns),
            axis(view.min[1], view.max[1], self.origin[1], self.rows),
        )
    }

    /// The typical distance between neighbouring cells.
    ///
    /// The bucket side is chosen from the density, so it is already that
    /// number; the panel uses it to size the dots.
    pub fn spacing(&self) -> f32 {
        self.cell
    }

    /// The cell nearest `at` within `radius`, or `None`.
    ///
    /// Every bucket the search disc touches is visited, so the answer is the
    /// one a scan of every cell would give. Sweeping only the bucket the
    /// pointer is in would miss a cell just across a bucket boundary — which
    /// is most of them, at the density this view is for.
    pub fn nearest(&self, points: &[Point], at: Point, radius: f32) -> Option<u32> {
        let disc = Bounds {
            min: [at[0] - radius, at[1] - radius],
            max: [at[0] + radius, at[1] + radius],
        };
        let (columns, rows) = self.span(disc);

        let mut best: Option<(u32, f32)> = None;
        for row in rows {
            for column in columns.clone() {
                for &candidate in &self.buckets[row * self.columns + column] {
                    let point = points[candidate as usize];
                    let (dx, dy) = (point[0] - at[0], point[1] - at[1]);
                    let distance = dx * dx + dy * dy;
                    if distance <= radius * radius
                        && best.map(|(_, best)| distance < best).unwrap_or(true)
                    {
                        best = Some((candidate, distance));
                    }
                }
            }
        }
        best.map(|(row, _)| row)
    }

    /// Every cell whose bucket meets `view`.
    ///
    /// Buckets, not cells: this is the culling pass, and it is allowed to
    /// return a cell just outside the rectangle. It must never miss one
    /// inside it.
    pub fn in_view(&self, view: Bounds) -> Vec<u32> {
        let (columns, rows) = self.span(view);
        let mut visible = Vec::new();
        for row in rows {
            for column in columns.clone() {
                visible.extend_from_slice(&self.buckets[row * self.columns + column]);
            }
        }
        visible
    }
}

// ---------------------------------------------------------------------------
// The camera
// ---------------------------------------------------------------------------

/// Maps the sample's coordinates onto the canvas.
///
/// `screen = world * scale + offset`, with y flipped: a nodes file counts y
/// upwards, a screen counts it downwards, and a network drawn without the flip
/// is a mirror image of the figure the pipeline wrote.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub scale: f32,
    pub offset: [f32; 2],
}

impl Camera {
    /// The camera that fits `bounds` into a viewport of `size`, with a small
    /// margin so the outermost cells are not cut in half.
    pub fn fit(bounds: Bounds, size: [f32; 2]) -> Self {
        // The smaller of the two ratios, so both axes fit; one scale for both,
        // so a round sample does not come out as an ellipse.
        let horizontal = size[0] / bounds.width().max(f32::MIN_POSITIVE);
        let vertical = size[1] / bounds.height().max(f32::MIN_POSITIVE);
        let scale = horizontal.min(vertical) * FIT_MARGIN;
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };

        // Place the sample's centre at the viewport's centre. The y term adds
        // rather than subtracts because the axis is flipped below.
        let centre = bounds.centre();
        Self {
            scale,
            offset: [
                size[0] * 0.5 - centre[0] * scale,
                size[1] * 0.5 + centre[1] * scale,
            ],
        }
    }

    pub fn world_to_screen(&self, point: Point) -> Point {
        let flipped = flip(point);
        [
            flipped[0] * self.scale + self.offset[0],
            flipped[1] * self.scale + self.offset[1],
        ]
    }

    pub fn screen_to_world(&self, point: Point) -> Point {
        [
            (point[0] - self.offset[0]) / self.scale,
            (self.offset[1] - point[1]) / self.scale,
        ]
    }

    /// Zoom by `factor`, keeping whatever is under `anchor` under it.
    ///
    /// Zooming about the centre instead is the difference between a view you
    /// can steer and one you fight.
    pub fn zoom_at(&mut self, factor: f32, anchor: Point) {
        let before = self.screen_to_world(anchor);
        self.scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        let after = self.screen_to_world(anchor);
        // Put back the world distance the zoom moved under the pointer.
        self.offset[0] += (after[0] - before[0]) * self.scale;
        self.offset[1] -= (after[1] - before[1]) * self.scale;
    }

    pub fn pan(&mut self, delta: [f32; 2]) {
        self.offset[0] += delta[0];
        self.offset[1] += delta[1];
    }

    /// The part of the sample a viewport of `size` is showing.
    pub fn visible(&self, size: [f32; 2]) -> Bounds {
        // The screen's top-left and bottom-right, back in world coordinates.
        // The flip swaps which is which vertically, so the two are sorted
        // rather than assumed.
        let a = self.screen_to_world([0.0, 0.0]);
        let b = self.screen_to_world(size);
        Bounds {
            min: [a[0].min(b[0]), a[1].min(b[1])],
            max: [a[0].max(b[0]), a[1].max(b[1])],
        }
    }
}

/// A world point in mesh space.
///
/// The mesh the panel caches is built in this space, and the camera then
/// reduces to a scale and a translation — which is exactly what a layer
/// transform can express, and why panning costs nothing: the same mesh is
/// re-used and only the transform moves.
///
/// The flip is the whole of it. A nodes file counts y upwards and a screen
/// counts it downwards; a transform cannot mirror, so the mirror is baked into
/// the mesh once instead.
pub fn flip(point: Point) -> Point {
    [point[0], -point[1]]
}

/// How much of the viewport a fitted sample takes: a little under all of it,
/// so the outermost cells are drawn whole.
const FIT_MARGIN: f32 = 0.92;

/// The zoom range. The bottom stops the sample vanishing into a dot; the top
/// stops the coordinates losing precision as `f32`.
const MIN_SCALE: f32 = 1e-4;
const MAX_SCALE: f32 = 1e5;

/// Smallest and largest a cell may be drawn, in pixels.
const MIN_RADIUS: f32 = 0.5;
const MAX_RADIUS: f32 = 6.0;

/// Fewest and most sides a cell's disc is built from.
///
/// Six at the bottom, not four: a square rotated forty-five degrees is a
/// diamond and reads as one, while a hexagon reads as a dot from two pixels up.
const MIN_SEGMENTS: usize = 6;
const MAX_SEGMENTS: usize = 16;

/// The radius, in pixels, to draw a cell at.
///
/// Tied to the distance between cells rather than to the zoom alone: a dot
/// that keeps growing swallows its neighbours, and one that keeps shrinking
/// disappears. A third of the spacing leaves the network visible between the
/// cells; the clamp keeps a dot hittable when zoomed out and modest when
/// zoomed in.
pub fn point_radius(scale: f32, spacing: f32) -> f32 {
    (spacing * scale / 3.0).clamp(MIN_RADIUS, MAX_RADIUS)
}

/// How many sides to build a cell's disc out of.
///
/// A disc is a fan of triangles, and the panel draws up to sixty thousand of
/// them, so the count follows the radius instead of being fixed. At half a
/// pixel a hexagon and a circle are the same handful of pixels; at six a
/// hexagon is visibly a hexagon. The two ends of the range are also the two
/// ends of the cost: the widest views are the ones with the most cells in
/// them, and those are the ones drawn at the smallest radius.
pub fn circle_segments(radius: f32) -> usize {
    // Three sides per pixel of radius, which saturates the cap exactly at the
    // largest a cell is ever drawn and bottoms out well before the smallest.
    ((radius * 3.0).ceil() as usize).clamp(MIN_SEGMENTS, MAX_SEGMENTS)
}

/// At most this many cells go into one mesh.
///
/// Past it the cells are drawn every `stride`-th. At a zoom where sixty
/// thousand cells share the canvas each one is under a pixel and they overlap
/// several deep, so the subsample is the same picture for a third of the
/// vertices — and the vertices are what is copied to the GPU every frame.
pub const CELL_BUDGET: usize = 60_000;

/// Draw one cell in every `stride_for(n)`.
pub fn stride_for(cells: usize) -> usize {
    cells.div_ceil(CELL_BUDGET).max(1)
}

/// The region a mesh is built for, given what is on screen.
///
/// Half a screen wider on every side, so that panning does not rebuild the
/// mesh at every frame — it rebuilds when the view leaves what was built,
/// which at a normal drag is a few times a second rather than sixty.
pub fn mesh_region(visible: Bounds) -> Bounds {
    visible.expanded(visible.width().max(visible.height()) * 0.5)
}

/// Whether a mesh built for `built` still covers everything `visible` shows.
pub fn covers(built: Bounds, visible: Bounds) -> bool {
    built.min[0] <= visible.min[0]
        && built.min[1] <= visible.min[1]
        && built.max[0] >= visible.max[0]
        && built.max[1] >= visible.max[1]
}

// ---------------------------------------------------------------------------
// Several columns at once
// ---------------------------------------------------------------------------

/// How many columns the tab can colour by at once.
///
/// Four. Not a technical ceiling — the cost is a colour lookup per cell per
/// column — but a legible one: a blend of four hues is already a colour no
/// legend can name, and a fifth would only make it harder to say which column
/// put the colour there.
pub const MAX_LAYERS: usize = 4;

/// What one column contributes to a cell: the colour it would give that cell
/// on its own, and how strongly it applies.
///
/// `None` when the column has no value for that cell.
pub type Channel = Option<(Rgb, f32)>;

/// Blend the columns' contributions into the one colour a cell is drawn in.
///
/// A weighted mean, the weights being how strongly each column is expressed.
/// Which means:
///
/// * one column is unchanged — a mean of one thing is that thing, so choosing
///   a single column draws exactly what it drew before there were four;
/// * a column that dominates a cell gives the cell its colour, including its
///   darkness, so a strong marker still reads as strong;
/// * two columns equally strong give the colour between them, which is the
///   whole point: a cell high in the red column and the blue one is neither
///   red nor blue;
/// * a cell low in everything is left at the pale end of the ramps, so it
///   recedes exactly as it does with one column.
///
/// # What it cannot do
///
/// A blend cannot be read back. There is no colour bar for "half red, half
/// blue", and there is no way to recover two values from one colour — the bars
/// in the margin describe each column *alone*, and the exact values are on the
/// tooltip. That is the price of one picture instead of four, and it is the
/// reason the tooltip lists every coloured column rather than only the one
/// under the pointer.
///
/// A cell with no value in any of the columns is [`Gradient::BAD`] — the same
/// grey the figures use — so "no data" never reads as "low".
pub fn fuse(channels: &[Channel]) -> Rgb {
    let mut present = 0usize;
    let mut total = 0.0f32;
    for (_, weight) in channels.iter().flatten() {
        present += 1;
        total += weight.max(0.0);
    }
    if present == 0 {
        return Gradient::BAD;
    }

    // Every column present but every one of them at the bottom of its range:
    // there is no expression to weigh, so the columns are given equal say and
    // the cell comes out at the pale end, which is the truth about it.
    let flat = total <= f32::EPSILON;
    let total = if flat { present as f32 } else { total };

    let mut blended = [0.0f32; 3];
    for (colour, weight) in channels.iter().flatten() {
        let weight = if flat { 1.0 } else { weight.max(0.0) };
        for (sum, value) in blended.iter_mut().zip(*colour) {
            *sum += f32::from(value) * weight;
        }
    }
    blended.map(|sum| (sum / total).round().clamp(0.0, 255.0) as u8)
}

// ---------------------------------------------------------------------------
// A loaded sample
// ---------------------------------------------------------------------------

/// One sample's network, loaded and ready to draw.
#[derive(Debug)]
pub struct NetworkSample {
    pub positions: Vec<Point>,
    /// Empty when step 1 has not produced the edges file yet: the cells are
    /// still worth showing on their own.
    pub edges: Vec<(u32, u32)>,
    pub bounds: Bounds,
    /// Every column of the nodes file, offered to the pickers.
    pub columns: Vec<String>,
    pub index: SpatialIndex,
    table: Table,
}

impl NetworkSample {
    /// Read a sample's nodes, and its edges when they exist.
    pub fn load(
        nodes: &Path,
        edges: Option<&Path>,
        extension: Extension,
        x_column: &str,
        y_column: &str,
    ) -> anyhow::Result<Self> {
        let table = read_table(nodes, extension)?;
        let positions: Vec<Point> = table
            .coords(x_column, y_column)?
            .into_iter()
            .map(|[x, y]| [x as f32, y as f32])
            .collect();

        let bounds = Bounds::of(&positions).ok_or_else(|| {
            anyhow::anyhow!(
                "{} has no cell with usable coordinates in `{x_column}` and `{y_column}`",
                nodes.display()
            )
        })?;

        let edges = match edges {
            Some(path) => {
                let pairs = read_table(path, extension)?.edges()?;
                // An endpoint past the end of the sample would index out of
                // the position array inside the paint loop. Better to refuse
                // the file, naming it, than to panic while drawing.
                let n_cells = positions.len() as u32;
                if let Some((a, b)) = pairs.iter().find(|(a, b)| *a >= n_cells || *b >= n_cells) {
                    anyhow::bail!(
                        "{} has an edge {a}–{b}, and {} only has {n_cells} cells",
                        path.display(),
                        nodes.display()
                    );
                }
                pairs
            }
            None => Vec::new(),
        };

        let columns = table
            .column_names()
            .into_iter()
            .map(str::to_string)
            .collect();
        let index = SpatialIndex::build(&positions, bounds);

        Ok(Self {
            positions,
            edges,
            bounds,
            columns,
            index,
            table,
        })
    }

    pub fn n_cells(&self) -> usize {
        self.positions.len()
    }

    /// Read a column as something to colour by.
    pub fn attribute(&self, column: &str) -> anyhow::Result<Attribute> {
        Attribute::read(&self.table, column)
    }

    /// Read a column as text, for the tooltip.
    pub fn text_column(&self, column: &str) -> anyhow::Result<Vec<String>> {
        Ok(self.table.string_column(column)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(side: usize) -> Vec<Point> {
        (0..side * side)
            .map(|i| [(i % side) as f32, (i / side) as f32])
            .collect()
    }

    // -----------------------------------------------------------------------
    // Bounds
    // -----------------------------------------------------------------------

    #[test]
    fn bounds_cover_every_point() {
        let bounds = Bounds::of(&[[0.0, 1.0], [4.0, -2.0], [2.0, 3.0]]).unwrap();
        assert_eq!(bounds.min, [0.0, -2.0]);
        assert_eq!(bounds.max, [4.0, 3.0]);
        assert_eq!(bounds.width(), 4.0);
        assert_eq!(bounds.height(), 5.0);
    }

    /// One unreadable coordinate must not make the whole view unopenable.
    #[test]
    fn bounds_ignore_points_that_are_not_numbers() {
        let bounds = Bounds::of(&[[0.0, 0.0], [f32::NAN, 5.0], [2.0, 2.0]]).unwrap();
        assert_eq!(bounds.min, [0.0, 0.0]);
        assert_eq!(bounds.max, [2.0, 2.0]);
    }

    #[test]
    fn nothing_has_no_bounds() {
        assert!(Bounds::of(&[]).is_none());
        assert!(Bounds::of(&[[f32::NAN, f32::NAN]]).is_none());
    }

    // -----------------------------------------------------------------------
    // Columns
    // -----------------------------------------------------------------------

    #[test]
    fn a_text_column_is_a_vocabulary() {
        let table = Table::from_columns(vec![(
            "phenotype".into(),
            Table::string_array(["cancer", "immune", "cancer"]),
        )])
        .unwrap();

        let attribute = Attribute::read(&table, "phenotype").unwrap();
        match &attribute {
            Attribute::Categorical { levels, codes } => {
                assert_eq!(levels, &["cancer", "immune"], "in first-seen order");
                assert_eq!(codes, &[0, 1, 0]);
            }
            other => panic!("expected labels, got {other:?}"),
        }
        assert_eq!(attribute.text(1), "immune");
    }

    /// `niches` is written back as a float, and is still a label.
    #[test]
    fn a_column_of_whole_numbers_with_few_values_is_a_vocabulary() {
        let table = Table::from_columns(vec![(
            "niches".into(),
            Table::f64_array([0.0, 2.0, 1.0, 2.0]),
        )])
        .unwrap();

        match Attribute::read(&table, "niches").unwrap() {
            Attribute::Categorical { levels, codes } => {
                assert_eq!(levels.len(), 3);
                assert_eq!(codes.len(), 4);
                assert_eq!(codes[1], codes[3], "the same niche twice");
            }
            other => panic!("a niche label is not a measurement: {other:?}"),
        }
    }

    /// A biomarker is a measurement, however few cells there are.
    #[test]
    fn a_column_of_fractions_is_a_measurement() {
        let table =
            Table::from_columns(vec![("CD8".into(), Table::f64_array([0.5, 1.25, 0.75]))]).unwrap();

        match Attribute::read(&table, "CD8").unwrap() {
            Attribute::Continuous { min, max, .. } => {
                assert_eq!((min, max), (0.5, 1.25));
            }
            other => panic!("expected a measurement, got {other:?}"),
        }
    }

    /// And so is a whole-numbered column with too many values to label.
    #[test]
    fn too_many_whole_numbers_is_a_measurement() {
        let counts: Vec<f64> = (0..MAX_LEVELS as i32 + 1).map(f64::from).collect();
        let table = Table::from_columns(vec![("count".into(), Table::f64_array(counts))]).unwrap();
        assert!(matches!(
            Attribute::read(&table, "count").unwrap(),
            Attribute::Continuous { .. }
        ));
    }

    #[test]
    fn a_missing_column_is_an_error() {
        let table = Table::from_columns(vec![("x".into(), Table::f64_array([1.0]))]).unwrap();
        assert!(Attribute::read(&table, "CD8").is_err());
    }

    // -----------------------------------------------------------------------
    // Colours and legend
    // -----------------------------------------------------------------------

    #[test]
    fn every_level_gets_its_own_colour() {
        let table = Table::from_columns(vec![(
            "phenotype".into(),
            Table::string_array(["a", "b", "c", "a"]),
        )])
        .unwrap();
        let attribute = Attribute::read(&table, "phenotype").unwrap();

        assert_eq!(attribute.colour(0, Palette::Reds), attribute.colour(3, Palette::Reds), "same label");
        assert_ne!(attribute.colour(0, Palette::Reds), attribute.colour(1, Palette::Reds));

        match attribute.legend(Palette::Reds) {
            Legend::Categories(entries) => {
                assert_eq!(entries.len(), 3);
                assert_eq!(entries[0].0, "a");
                assert_eq!(entries[0].1, attribute.colour(0, Palette::Reds), "the legend must not lie");
            }
            other => panic!("expected a list, got {other:?}"),
        }
    }

    #[test]
    fn a_measurement_gets_a_colour_bar_with_its_range() {
        let table =
            Table::from_columns(vec![("CD8".into(), Table::f64_array([0.5, 1.5, 2.5]))]).unwrap();
        let attribute = Attribute::read(&table, "CD8").unwrap();

        match attribute.legend(Palette::Reds) {
            Legend::Colorbar { ramp, min, max } => {
                assert_eq!((min, max), (0.5, 2.5));
                assert_eq!(ramp.len(), RAMP_STEPS);
                assert_eq!(ramp[0], attribute.colour(0, Palette::Reds), "the low end is the low value");
                assert_eq!(*ramp.last().unwrap(), attribute.colour(2, Palette::Reds));
            }
            other => panic!("expected a colour bar, got {other:?}"),
        }
    }

    /// A column with one value repeated has no range to spread a ramp over.
    /// It must still draw, in one colour, rather than divide by zero.
    #[test]
    fn a_flat_measurement_still_has_a_colour() {
        let table =
            Table::from_columns(vec![("flat".into(), Table::f64_array([2.5, 2.5]))]).unwrap();
        let attribute = Attribute::read(&table, "flat").unwrap();

        let colour = attribute.colour(0, Palette::Reds);
        assert_eq!(colour, attribute.colour(1, Palette::Reds), "one value, one colour");
        assert_ne!(
            colour,
            Gradient::BAD,
            "a value that is present must not be drawn as a missing one"
        );
        assert_eq!(colour, reds().sample(0.0), "the bottom of the ramp");
    }

    /// A cell with no measurement is not a cell measuring zero.
    #[test]
    fn a_missing_measurement_is_greyed_rather_than_read_as_zero() {
        let table = Table::from_columns(vec![(
            "CD8".into(),
            Table::f64_array([0.25, f64::NAN, 10.5]),
        )])
        .unwrap();
        let attribute = Attribute::read(&table, "CD8").unwrap();

        assert_eq!(attribute.colour(1, Palette::Reds), Gradient::BAD);
        assert_ne!(attribute.colour(0, Palette::Reds), attribute.colour(1, Palette::Reds));
        assert_eq!(attribute.text(1), "—");
    }

    /// And a hole in a column of labels is not a label.
    ///
    /// A `NaN` renders as the string `NaN`, so reading the levels off the text
    /// would invent a niche called "not a number", give it a colour and a line
    /// in the legend, and count it among the niches.
    #[test]
    fn a_missing_label_does_not_become_a_level_of_its_own() {
        let table = Table::from_columns(vec![(
            "niches".into(),
            Table::f64_array([0.0, f64::NAN, 1.0, 0.0]),
        )])
        .unwrap();
        let attribute = Attribute::read(&table, "niches").unwrap();

        match &attribute {
            Attribute::Categorical { levels, .. } => {
                assert_eq!(levels.len(), 2, "two niches, not three: {levels:?}");
                assert!(!levels.iter().any(|level| level.contains("NaN")));
            }
            other => panic!("expected labels, got {other:?}"),
        }
        assert_eq!(attribute.colour(1, Palette::Reds), Gradient::BAD);
        assert_eq!(attribute.text(1), "—");
        assert_eq!(attribute.colour(0, Palette::Reds), attribute.colour(3, Palette::Reds));

        match attribute.legend(Palette::Reds) {
            Legend::Categories(entries) => assert_eq!(entries.len(), 2),
            other => panic!("expected a list, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Picking
    // -----------------------------------------------------------------------

    #[test]
    fn the_index_finds_the_cell_under_the_pointer() {
        let points = grid(20);
        let index = SpatialIndex::build(&points, Bounds::of(&points).unwrap());

        let found = index.nearest(&points, [7.1, 12.2], 1.0).unwrap();
        assert_eq!(points[found as usize], [7.0, 12.0]);
    }

    #[test]
    fn the_index_finds_nothing_out_in_the_open() {
        let points = grid(10);
        let index = SpatialIndex::build(&points, Bounds::of(&points).unwrap());
        assert_eq!(index.nearest(&points, [50.0, 50.0], 1.0), None);
    }

    /// The whole point of the grid: the answer must be the same as a linear
    /// scan would give, at every position, or the tooltip names another cell.
    #[test]
    fn the_index_agrees_with_a_linear_scan() {
        let points = grid(15);
        let bounds = Bounds::of(&points).unwrap();
        let index = SpatialIndex::build(&points, bounds);
        let radius = 0.7;

        for i in 0..40 {
            let at = [i as f32 * 0.37, i as f32 * 0.53];
            let scanned = points
                .iter()
                .enumerate()
                .map(|(row, p)| {
                    let (dx, dy) = (p[0] - at[0], p[1] - at[1]);
                    (row, dx * dx + dy * dy)
                })
                .filter(|(_, d2)| *d2 <= radius * radius)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(row, _)| row as u32);
            assert_eq!(index.nearest(&points, at, radius), scanned, "at {at:?}");
        }
    }

    #[test]
    fn culling_keeps_everything_inside_the_view() {
        let points = grid(20);
        let index = SpatialIndex::build(&points, Bounds::of(&points).unwrap());
        let view = Bounds {
            min: [5.0, 5.0],
            max: [9.0, 9.0],
        };

        let visible = index.in_view(view);
        for (row, point) in points.iter().enumerate() {
            let inside = point[0] >= view.min[0]
                && point[0] <= view.max[0]
                && point[1] >= view.min[1]
                && point[1] <= view.max[1];
            if inside {
                assert!(
                    visible.contains(&(row as u32)),
                    "{point:?} is in view and was culled"
                );
            }
        }
        assert!(
            visible.len() < points.len(),
            "culling that keeps everything is not culling"
        );
    }

    // -----------------------------------------------------------------------
    // Camera
    // -----------------------------------------------------------------------

    #[test]
    fn fitting_puts_the_whole_sample_on_the_canvas() {
        let bounds = Bounds {
            min: [0.0, 0.0],
            max: [100.0, 50.0],
        };
        let camera = Camera::fit(bounds, [400.0, 400.0]);

        for corner in [bounds.min, bounds.max, [0.0, 50.0], [100.0, 0.0]] {
            let [x, y] = camera.world_to_screen(corner);
            assert!(
                (0.0..=400.0).contains(&x) && (0.0..=400.0).contains(&y),
                "{corner:?} landed off-canvas at {x}, {y}"
            );
        }
    }

    #[test]
    fn fitting_keeps_the_shape_of_the_sample() {
        let bounds = Bounds {
            min: [0.0, 0.0],
            max: [100.0, 50.0],
        };
        let camera = Camera::fit(bounds, [400.0, 400.0]);

        let width = camera.world_to_screen([100.0, 0.0])[0] - camera.world_to_screen([0.0, 0.0])[0];
        let height =
            (camera.world_to_screen([0.0, 50.0])[1] - camera.world_to_screen([0.0, 0.0])[1]).abs();
        assert!(
            (width / height - 2.0).abs() < 1e-3,
            "a sample twice as wide as it is tall came out {width} by {height}"
        );
    }

    /// A nodes file counts y upwards; a screen counts it downwards.
    #[test]
    fn the_vertical_axis_is_flipped() {
        let camera = Camera::fit(
            Bounds {
                min: [0.0, 0.0],
                max: [10.0, 10.0],
            },
            [100.0, 100.0],
        );
        let low = camera.world_to_screen([5.0, 0.0]);
        let high = camera.world_to_screen([5.0, 10.0]);
        assert!(high[1] < low[1], "the sample is upside down");
    }

    #[test]
    fn screen_and_world_are_inverses() {
        let camera = Camera {
            scale: 3.5,
            offset: [12.0, -30.0],
        };
        for point in [[0.0, 0.0], [17.5, -4.25], [1000.0, 900.0]] {
            let round_trip = camera.screen_to_world(camera.world_to_screen(point));
            assert!(
                (round_trip[0] - point[0]).abs() < 1e-3 && (round_trip[1] - point[1]).abs() < 1e-3,
                "{point:?} came back as {round_trip:?}"
            );
        }
    }

    /// Zooming under the pointer is what makes the view steerable.
    #[test]
    fn zooming_keeps_what_is_under_the_pointer_under_it() {
        let mut camera = Camera {
            scale: 2.0,
            offset: [50.0, 50.0],
        };
        let anchor = [123.0, 77.0];
        let before = camera.screen_to_world(anchor);

        camera.zoom_at(2.5, anchor);
        let after = camera.screen_to_world(anchor);

        assert!(
            (before[0] - after[0]).abs() < 1e-2 && (before[1] - after[1]).abs() < 1e-2,
            "the point under the pointer moved from {before:?} to {after:?}"
        );
        assert!((camera.scale - 5.0).abs() < 1e-4);
    }

    #[test]
    fn the_visible_rectangle_is_what_the_camera_maps_onto_the_canvas() {
        let camera = Camera::fit(
            Bounds {
                min: [0.0, 0.0],
                max: [100.0, 100.0],
            },
            [200.0, 200.0],
        );
        let visible = camera.visible([200.0, 200.0]);

        assert!(visible.min[0] <= 0.0 && visible.min[1] <= 0.0);
        assert!(visible.max[0] >= 100.0 && visible.max[1] >= 100.0);
    }

    // -----------------------------------------------------------------------
    // Level of detail
    // -----------------------------------------------------------------------

    /// A dot must stay hittable at any zoom, and never swallow its neighbour.
    #[test]
    fn a_cell_is_drawn_between_half_a_pixel_and_a_few() {
        for scale in [0.001, 0.05, 1.0, 20.0, 500.0] {
            let radius = point_radius(scale, 12.0);
            assert!(
                (0.5..=6.0).contains(&radius),
                "at scale {scale} a cell is {radius} px"
            );
        }
    }

    #[test]
    fn a_cell_grows_with_the_zoom_until_it_is_capped() {
        assert!(point_radius(1.0, 12.0) > point_radius(0.1, 12.0));
    }

    /// A dot is a disc, and the disc's cost follows its size.
    #[test]
    fn a_small_cell_is_built_from_fewer_sides_than_a_large_one() {
        assert_eq!(circle_segments(MIN_RADIUS), MIN_SEGMENTS);
        assert_eq!(circle_segments(MAX_RADIUS), MAX_SEGMENTS);
        assert!(circle_segments(3.0) > circle_segments(1.0));

        // Never so few that the dot reads as a polygon, never so many that a
        // wide view pays for detail below a pixel.
        for radius in [0.1, 0.5, 1.0, 2.5, 6.0, 100.0] {
            let sides = circle_segments(radius);
            assert!(
                (MIN_SEGMENTS..=MAX_SEGMENTS).contains(&sides),
                "a {radius} px cell got {sides} sides"
            );
        }
    }

    /// The two ways of asking for a colour must agree, or the legend and the
    /// canvas would disagree about what a cell is.
    /// A table with a column of labels and a measurement that is missing in
    /// one row — the two things a cell's colour can come from, and the row
    /// where they have to agree that there is nothing there.
    fn table_of_columns() -> Table {
        Table::from_columns(vec![
            (
                "phenotype".into(),
                Table::string_array(["a", "b", "c", "a"]),
            ),
            ("CD8".into(), Table::f64_array([0.25, 1.5, f64::NAN, 9.75])),
        ])
        .unwrap()
    }

    #[test]
    fn the_resolved_map_gives_the_same_colours_as_the_single_lookups() {
        let table = table_of_columns();

        for column in ["phenotype", "CD8"] {
            let attribute = Attribute::read(&table, column).unwrap();
            let colouring = attribute.colouring(Palette::Reds);
            for row in 0..5 {
                assert_eq!(
                    attribute.colour(row, Palette::Reds),
                    colouring.colour(row),
                    "`{column}` disagrees about row {row}"
                );
            }
        }
    }

    /// The contract the panel leans on: the camera is a scale and a
    /// translation applied to the flipped point, and nothing else. If that
    /// stopped being true, the cached mesh could no longer be moved by a layer
    /// transform and every pan would rebuild it.
    #[test]
    fn the_camera_is_a_scale_and_a_translation_of_the_flipped_point() {
        let camera = Camera {
            scale: 2.5,
            offset: [30.0, -7.0],
        };
        for point in [[0.0, 0.0], [10.0, -4.0], [-3.5, 8.25]] {
            let mesh = flip(point);
            let expected = [
                mesh[0] * camera.scale + camera.offset[0],
                mesh[1] * camera.scale + camera.offset[1],
            ];
            assert_eq!(camera.world_to_screen(point), expected);
        }
    }

    #[test]
    fn a_sample_within_budget_is_drawn_whole() {
        assert_eq!(stride_for(0), 1);
        assert_eq!(stride_for(CELL_BUDGET), 1);
    }

    #[test]
    fn a_sample_over_budget_is_thinned_enough_to_fit() {
        for cells in [CELL_BUDGET + 1, 200_000, 1_000_000] {
            let stride = stride_for(cells);
            assert!(
                cells.div_ceil(stride) <= CELL_BUDGET,
                "{cells} cells at stride {stride} still draws {} of them",
                cells.div_ceil(stride)
            );
        }
    }

    /// A mesh is built wider than the screen so that panning re-uses it.
    #[test]
    fn the_mesh_covers_more_than_the_screen() {
        let visible = Bounds {
            min: [0.0, 0.0],
            max: [100.0, 100.0],
        };
        let region = mesh_region(visible);

        assert!(covers(region, visible));
        assert!(region.width() > visible.width() * 1.5);

        // Panning by a quarter of a screen must not rebuild it...
        let nudged = Bounds {
            min: [25.0, 25.0],
            max: [125.0, 125.0],
        };
        assert!(covers(region, nudged));

        // ...and panning past what was built must.
        let far = Bounds {
            min: [200.0, 0.0],
            max: [300.0, 100.0],
        };
        assert!(!covers(region, far));
    }

    // -----------------------------------------------------------------------
    // Loading
    // -----------------------------------------------------------------------

    fn write_sample(directory: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
        let nodes = Table::from_columns(vec![
            ("X".into(), Table::f64_array([0.0, 1.0, 2.0, 3.0])),
            ("Y".into(), Table::f64_array([0.0, 1.0, 0.0, 1.0])),
            (
                "phenotype".into(),
                Table::string_array(["a", "b", "a", "b"]),
            ),
            ("CD8".into(), Table::f64_array([0.1, 0.9, 0.4, 0.6])),
        ])
        .unwrap();
        let nodes_path = directory.join("nodes_patient-1.parquet");
        mosna_io::write::write_parquet::write_parquet(&nodes, &nodes_path).unwrap();

        let edges = Table::from_edges(&[(0, 1), (1, 2), (2, 3)]).unwrap();
        let edges_path = directory.join("edges_patient-1.parquet");
        mosna_io::write::write_parquet::write_parquet(&edges, &edges_path).unwrap();

        (nodes_path, edges_path)
    }

    #[test]
    fn a_sample_loads_its_cells_its_edges_and_its_columns() {
        let directory = tempfile::tempdir().unwrap();
        let (nodes, edges) = write_sample(directory.path());

        let sample =
            NetworkSample::load(&nodes, Some(&edges), Extension::Parquet, "X", "Y").unwrap();

        assert_eq!(sample.n_cells(), 4);
        assert_eq!(sample.edges, vec![(0, 1), (1, 2), (2, 3)]);
        assert_eq!(sample.bounds.min, [0.0, 0.0]);
        assert_eq!(sample.bounds.max, [3.0, 1.0]);
        assert!(sample.columns.contains(&"CD8".to_string()));
        assert!(matches!(
            sample.attribute("phenotype").unwrap(),
            Attribute::Categorical { .. }
        ));
        assert_eq!(sample.text_column("phenotype").unwrap()[1], "b");
    }

    /// Step 1 may not have run yet. The cells are still worth looking at.
    #[test]
    fn a_sample_without_edges_still_loads() {
        let directory = tempfile::tempdir().unwrap();
        let (nodes, _) = write_sample(directory.path());

        let sample = NetworkSample::load(&nodes, None, Extension::Parquet, "X", "Y").unwrap();
        assert_eq!(sample.n_cells(), 4);
        assert!(sample.edges.is_empty());
    }

    /// An edge pointing at a cell that does not exist would index out of the
    /// position array while drawing — in the paint loop, on a user's machine.
    #[test]
    fn an_edge_pointing_outside_the_sample_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let (nodes, _) = write_sample(directory.path());
        let edges = Table::from_edges(&[(0, 9)]).unwrap();
        let edges_path = directory.path().join("edges_bad.parquet");
        mosna_io::write::write_parquet::write_parquet(&edges, &edges_path).unwrap();

        let error = NetworkSample::load(&nodes, Some(&edges_path), Extension::Parquet, "X", "Y")
            .unwrap_err();
        assert!(error.to_string().contains('9'), "{error}");
    }

    #[test]
    fn a_missing_coordinate_column_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        let (nodes, _) = write_sample(directory.path());
        let error =
            NetworkSample::load(&nodes, None, Extension::Parquet, "x_pos", "Y").unwrap_err();
        assert!(error.to_string().contains("x_pos"), "{error}");
    }

    // -----------------------------------------------------------------------
    // Palettes and the tiles they are drawn in
    // -----------------------------------------------------------------------

    /// Adding a column must not hand it a colour another view already has.
    #[test]
    fn every_view_opens_on_a_palette_of_its_own() {
        let handed: Vec<Palette> = (0..MAX_LAYERS).map(Palette::nth).collect();
        for (index, palette) in handed.iter().enumerate() {
            assert!(
                !handed[index + 1..].contains(palette),
                "{} was handed out twice",
                palette.label()
            );
        }
    }

    #[test]
    fn a_palette_names_itself_and_produces_a_ramp() {
        for palette in Palette::ALL {
            assert!(!palette.label().is_empty());
            assert_eq!(palette.ramp().len(), RAMP_STEPS);
            assert_eq!(palette.ramp()[0], palette.gradient().sample(0.0));
            assert_eq!(*palette.ramp().last().unwrap(), palette.gradient().sample(1.0));
        }
    }

    /// The palette is what the picker changes, so changing it has to change
    /// the picture — for a measurement.
    #[test]
    fn a_measured_column_changes_colour_with_its_palette() {
        let attribute = Attribute::Continuous {
            values: vec![0.0, 1.0],
            min: 0.0,
            max: 1.0,
        };
        assert_ne!(
            attribute.colour(1, Palette::Reds),
            attribute.colour(1, Palette::Blues)
        );
    }

    /// And a column of labels has to ignore it: a niche's colour is shared
    /// with every figure the pipeline writes, and a view that recoloured it
    /// would be disagreeing with them.
    #[test]
    fn a_labelled_column_ignores_the_palette() {
        let attribute = Attribute::labels(vec![Some("a".into()), Some("b".into())]);
        for palette in Palette::ALL {
            assert_eq!(
                attribute.colour(0, palette),
                attribute.colour(0, Palette::Reds),
                "{} recoloured a label",
                palette.label()
            );
        }
    }

    // -----------------------------------------------------------------------
    // Blending several columns into one picture
    // -----------------------------------------------------------------------

    fn channel(colour: Rgb, weight: f32) -> Channel {
        Some((colour, weight))
    }

    /// One column has to draw exactly what it drew before there were four, at
    /// any weight: a mean of one thing is that thing.
    #[test]
    fn one_column_is_left_exactly_as_it_was() {
        for weight in [0.0, 0.01, 0.5, 1.0] {
            assert_eq!(
                fuse(&[channel([0x67, 0x00, 0x0d], weight)]),
                [0x67, 0x00, 0x0d],
                "a single column was changed at weight {weight}"
            );
        }
    }

    /// Two columns equally strong give the colour between them — which is the
    /// whole point of blending rather than picking one.
    #[test]
    fn two_equal_columns_give_the_colour_between_them() {
        let red = [0xC0, 0x20, 0x20];
        let blue = [0x20, 0x20, 0xC0];
        let blended = fuse(&[channel(red, 1.0), channel(blue, 1.0)]);

        assert_eq!(blended, [0x70, 0x20, 0x70]);
        assert_ne!(blended, red);
        assert_ne!(blended, blue);
    }

    /// A column that dominates a cell gives the cell its colour, so a strong
    /// marker still reads as that marker and not as a muddle.
    #[test]
    fn the_dominant_column_carries_the_cell() {
        let red = [0xC0, 0x20, 0x20];
        let blue = [0x20, 0x20, 0xC0];
        let blended = fuse(&[channel(red, 1.0), channel(blue, 0.02)]);

        let distance = |a: Rgb, b: Rgb| (0..3).map(|c| (a[c] as i32 - b[c] as i32).abs()).sum::<i32>();
        assert!(
            distance(blended, red) < distance(blended, blue) / 4,
            "{blended:?} is not close enough to the column that dominates it"
        );
    }

    /// A cell low in every column stays at the pale end of the ramps, so it
    /// recedes exactly as it does with one column — and does not divide by a
    /// total weight of nothing on the way.
    #[test]
    fn a_cell_low_in_everything_stays_pale() {
        let pale = [0xFF, 0xF5, 0xF0];
        let blended = fuse(&[channel(pale, 0.0), channel(pale, 0.0)]);
        assert_eq!(blended, pale);
    }

    /// No value in any coloured column is grey, never a colour: "no data" must
    /// not read as "low".
    #[test]
    fn a_cell_with_nothing_in_it_is_the_missing_grey() {
        assert_eq!(fuse(&[None, None, None, None]), Gradient::BAD);
        assert_eq!(fuse(&[]), Gradient::BAD);
    }

    /// A column that is missing for *this* cell drops out of the blend rather
    /// than dragging it towards grey: three markers read and one not measured
    /// is still three markers.
    #[test]
    fn a_missing_column_leaves_the_others_alone() {
        let red = [0xC0, 0x20, 0x20];
        assert_eq!(fuse(&[channel(red, 1.0), None]), red);
    }

    /// The blend is a colour whatever it is handed — including the weights a
    /// column of one repeated value, or a range of zero, can produce.
    #[test]
    fn the_blend_survives_strange_weights() {
        let white = [255, 255, 255];
        let black = [0, 0, 0];

        // A negative weight drops its column out; it cannot pull the blend the
        // other way.
        assert_eq!(fuse(&[channel(white, -1.0), channel(black, 1.0)]), black);
        // No weight anywhere is equal say, not a division by nothing.
        assert_eq!(
            fuse(&[channel(white, 0.0), channel(black, 0.0)]),
            [128, 128, 128]
        );
        // And a vanishing weight beside a real one leaves the real one.
        assert_eq!(fuse(&[channel(white, 1e-9), channel(black, 1.0)]), black);
    }

    /// A measurement weighs where it sits in its own range, so a column in
    /// counts cannot shout down a column in fractions.
    #[test]
    fn a_measurement_weighs_its_place_in_its_own_range() {
        let attribute = Attribute::Continuous {
            values: vec![100.0, 200.0, 300.0, f64::NAN],
            min: 100.0,
            max: 300.0,
        };
        assert_eq!(attribute.weight(0), Some(0.0));
        assert_eq!(attribute.weight(1), Some(0.5));
        assert_eq!(attribute.weight(2), Some(1.0));
        assert_eq!(attribute.weight(3), None, "a missing value has no weight");
    }

    /// A label is a fact, not a degree: it weighs one, or it is not there.
    #[test]
    fn a_label_weighs_one() {
        let attribute = Attribute::labels(vec![Some("a".into()), None]);
        assert_eq!(attribute.weight(0), Some(1.0));
        assert_eq!(attribute.weight(1), None);
        assert_eq!(attribute.weight(9), None, "past the end is not a cell");
    }

    /// The weight and the colour have to agree about what is missing, or a
    /// cell drops out of the blend while still contributing a colour to it.
    #[test]
    fn the_weight_and_the_colour_agree_about_missing_values() {
        let table = table_of_columns();
        for column in ["phenotype", "CD8"] {
            let attribute = Attribute::read(&table, column).unwrap();
            for row in 0..attribute.len() {
                assert_eq!(
                    attribute.weight(row).is_none(),
                    attribute.colour(row, Palette::Reds) == Gradient::BAD,
                    "`{column}` disagrees about row {row}"
                );
            }
        }
    }
}
