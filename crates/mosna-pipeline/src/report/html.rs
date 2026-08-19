//! The page itself.
//!
//! One HTML file, written next to the figures it describes. It references them
//! by relative path rather than embedding them: the charts are four hundred
//! kilobytes each, twenty-four of them is ten megabytes, and a single file that
//! size opens slowly and edits worse. The report travels with its directory,
//! which is the unit anyone copying results moves anyway.
//!
//! No external resource is fetched — no font, no script, no stylesheet from a
//! network. These reports are read on machines that have none.

use std::path::Path;

use crate::report::layout;
use crate::report::tree::{Entry, Output};

/// What the page is made of.
pub struct Page<'a> {
    /// The directory being described, shown in the header.
    pub working_dir: &'a Path,
    /// When the report was made, already formatted.
    pub generated: &'a str,
    pub output: &'a Output,
}

/// Text, safe to put between tags or inside an attribute.
pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// A relative path, as a URL a browser will resolve.
///
/// Two separate jobs, and only the first belongs here: the separators become
/// forward slashes, and the characters that would change what the URL *means*
/// are percent-encoded. An ampersand is not one of them — it is legal in a path
/// — so it is left for [`escape`] to turn into an entity when the URL goes into
/// an attribute. Encoding it here would ask the server, or the file system, for
/// a file called `niche%26obs.png`, which does not exist.
pub fn href(path: &Path) -> String {
    let mut url = String::new();
    for character in path.display().to_string().chars() {
        match character {
            '\\' => url.push('/'),
            // `%` first, or the encodings below would be double-encoded.
            '%' => url.push_str("%25"),
            ' ' => url.push_str("%20"),
            '#' => url.push_str("%23"),
            '?' => url.push_str("%3F"),
            other => url.push(other),
        }
    }
    url
}

/// A size a person can read: `2.0 kB`, not `2048`.
fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["kB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }

    let mut value = bytes as f64 / 1000.0;
    let mut unit = UNITS[0];
    for next in &UNITS[1..] {
        if value < 1000.0 {
            break;
        }
        value /= 1000.0;
        unit = next;
    }
    format!("{value:.1} {unit}")
}

/// The whole document.
pub fn render(page: &Page) -> String {
    let output = page.output;
    let tabs = layout::tabs(output);
    let figures: usize = tabs.iter().map(layout::Tab::figures).sum();

    let mut html = String::with_capacity(64 * 1024);
    html.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<title>MOSNA report</title>\n");
    html.push_str(STYLE);
    html.push_str("</head>\n<body>\n");

    push_header(&mut html, page, figures);

    if tabs.is_empty() && output.tree.is_empty() {
        html.push_str(
            "<p class=\"empty\">No figure has been written to this directory yet. \
             Run an analysis and generate the report again.</p>\n",
        );
        html.push_str("</body>\n</html>\n");
        return html;
    }

    push_controls(&mut html, &tabs);

    for (index, tab) in tabs.iter().enumerate() {
        push_pane(&mut html, page, tab, index == 0);
    }
    push_files_pane(&mut html, output, tabs.is_empty());

    push_viewer(&mut html);
    html.push_str(SCRIPT);
    html.push_str("</body>\n</html>\n");
    html
}

fn push_header(html: &mut String, page: &Page, figures: usize) {
    let output = page.output;
    html.push_str("<header>\n<h1>MOSNA report</h1>\n");
    html.push_str(&format!(
        "<p class=\"where\"><span class=\"label\">Output directory</span> \
         <code>{}</code></p>\n",
        escape(&page.working_dir.display().to_string())
    ));
    html.push_str(&format!(
        "<p class=\"when\"><span class=\"label\">Generated</span> {}</p>\n",
        escape(page.generated)
    ));
    html.push_str(&format!(
        "<p class=\"summary\">{}, out of {} totalling {}.</p>\n",
        plural(figures, "figure", "figures"),
        plural(output.files, "file", "files"),
        human_bytes(output.bytes)
    ));
    html.push_str("</header>\n");
}

/// The tab strip and the search box, which stay put while the page scrolls.
fn push_controls(html: &mut String, tabs: &[layout::Tab]) {
    html.push_str("<div class=\"controls\">\n<div class=\"tabs\" role=\"tablist\">\n");
    for (index, tab) in tabs.iter().enumerate() {
        html.push_str(&format!(
            "<button class=\"tab\" type=\"button\" role=\"tab\" data-tab=\"{}\" \
             aria-selected=\"{}\">{} <span class=\"tab-count\" data-total=\"{}\">{}</span>\
             </button>\n",
            tab.id,
            index == 0,
            escape(tab.name),
            tab.figures(),
            tab.figures()
        ));
    }
    html.push_str(&format!(
        "<button class=\"tab\" type=\"button\" role=\"tab\" data-tab=\"files\" \
         aria-selected=\"{}\">Files</button>\n",
        tabs.is_empty()
    ));
    html.push_str("</div>\n");

    html.push_str(
        "<div class=\"find\">\n\
         <input id=\"search\" type=\"search\" autocomplete=\"off\" \
         placeholder=\"Filter by patient, sample or file name…\" \
         aria-label=\"Filter by patient, sample or file name\">\n\
         <span id=\"matches\" aria-live=\"polite\"></span>\n\
         </div>\n",
    );
    html.push_str("</div>\n");
}

fn push_pane(html: &mut String, page: &Page, tab: &layout::Tab, visible: bool) {
    html.push_str(&format!(
        "<div class=\"pane\" id=\"pane-{}\" data-pane=\"{}\" role=\"tabpanel\"{}>\n",
        tab.id,
        tab.id,
        if visible { "" } else { " hidden" }
    ));

    for group in &tab.groups {
        html.push_str(&format!(
            "<section class=\"group\" data-search=\"{}\">\n<h2>{}</h2>\n<div class=\"cards\">\n",
            escape(&group.search_key()),
            escape(&group.heading())
        ));
        for card in &group.cards {
            push_card(html, page, group, card);
        }
        html.push_str("</div>\n</section>\n");
    }

    html.push_str("<p class=\"nothing\" hidden>Nothing here matches what you typed.</p>\n");
    html.push_str("</div>\n");
}

/// One figure: a thumbnail that opens it, and the two files behind it.
///
/// The thumbnail is the image itself rather than a smaller copy of it. Making
/// real thumbnails would mean re-encoding every PNG, which is a decoder this
/// crate does not have and a dependency it does not want; `loading="lazy"`
/// costs nothing and defers everything below the fold, which is most of it.
fn push_card(html: &mut String, page: &Page, group: &layout::Group, card: &layout::Card) {
    let key = format!(
        "{} {} {}",
        card.stem,
        group.search_key(),
        card.directory.display()
    )
    .to_lowercase();

    html.push_str(&format!(
        "<figure class=\"card\" data-search=\"{}\"",
        escape(&key)
    ));
    html.push_str(&format!(" data-title=\"{}\"", escape(&card.stem)));
    if let Some(chart) = &card.chart {
        html.push_str(&format!(" data-chart=\"{}\"", escape(&href(chart))));
    }
    if let Some(image) = &card.image {
        html.push_str(&format!(" data-image=\"{}\"", escape(&href(image))));
    }
    // The proportions of the image, so the thumbnails of a row line up and the
    // page does not jump as they load.
    if let Some((width, height)) = card
        .image
        .as_ref()
        .and_then(|image| crate::report::png::dimensions(&page.working_dir.join(image)))
    {
        html.push_str(&format!(" style=\"--ratio: {width} / {height}\""));
    }
    html.push_str(">\n");

    html.push_str(&format!(
        "<button class=\"thumb\" type=\"button\" title=\"Open {}\">",
        escape(&card.stem)
    ));
    match &card.image {
        Some(image) => html.push_str(&format!(
            "<img src=\"{}\" alt=\"{}\" loading=\"lazy\" decoding=\"async\">",
            escape(&href(image)),
            escape(&card.stem)
        )),
        None => html.push_str("<span class=\"no-thumb\">Interactive chart</span>"),
    }
    html.push_str("</button>\n");

    html.push_str(&format!(
        "<figcaption>{}<span class=\"dir\">{}</span></figcaption>\n",
        escape(&card.stem),
        escape(&card.directory.display().to_string())
    ));

    html.push_str("<p class=\"links\">");
    if let Some(chart) = &card.chart {
        html.push_str(&format!("<a href=\"{}\">chart</a>", escape(&href(chart))));
    }
    if let Some(image) = &card.image {
        if card.chart.is_some() {
            html.push_str(" · ");
        }
        html.push_str(&format!("<a href=\"{}\">image</a>", escape(&href(image))));
    }
    html.push_str("</p>\n</figure>\n");
}

fn push_files_pane(html: &mut String, output: &Output, visible: bool) {
    html.push_str(&format!(
        "<div class=\"pane\" id=\"pane-files\" data-pane=\"files\" role=\"tabpanel\"{}>\n",
        if visible { "" } else { " hidden" }
    ));
    html.push_str("<h2>Everything in the directory</h2>\n<ul class=\"tree\">\n");
    push_entries(html, &output.tree);
    html.push_str("</ul>\n</div>\n");
}

/// Where a figure is opened, one at a time.
fn push_viewer(html: &mut String) {
    html.push_str(
        "<dialog id=\"viewer\">\n\
         <div class=\"viewer-bar\">\n\
         <strong id=\"viewer-title\"></strong>\n\
         <span class=\"viewer-actions\">\
         <a id=\"viewer-open\" href=\"#\" target=\"_blank\" rel=\"noopener\">open on its own</a>\
         <button id=\"viewer-close\" type=\"button\">Close</button>\
         </span>\n\
         </div>\n\
         <div id=\"viewer-body\"></div>\n\
         </dialog>\n",
    );
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn push_entries(html: &mut String, entries: &[Entry]) {
    for entry in entries {
        match entry {
            Entry::Directory { name, children } => {
                html.push_str(&format!(
                    "<li class=\"dir\"><details open><summary>{}</summary>\n<ul>\n",
                    escape(name)
                ));
                push_entries(html, children);
                html.push_str("</ul>\n</details></li>\n");
            }
            Entry::File { name, bytes } => html.push_str(&format!(
                "<li class=\"file\">{} <span class=\"size\">{}</span></li>\n",
                escape(name),
                human_bytes(*bytes)
            )),
        }
    }
}

/// Switching tabs, filtering, and opening a figure.
///
/// Inline, and written to run without anything else: a report is opened from a
/// memory stick, from a network share, from a machine with no internet. The
/// chart is put into the viewer only when a figure is opened, and taken out
/// again when it is closed — four hundred kilobytes live at a time instead of
/// ten megabytes at once.
const SCRIPT: &str = r#"<script>
(function () {
  var tabs = Array.prototype.slice.call(document.querySelectorAll('.tab'));
  var panes = Array.prototype.slice.call(document.querySelectorAll('.pane'));

  function show(id) {
    tabs.forEach(function (tab) {
      tab.setAttribute('aria-selected', String(tab.dataset.tab === id));
    });
    panes.forEach(function (pane) { pane.hidden = pane.dataset.pane !== id; });
  }
  tabs.forEach(function (tab) {
    tab.addEventListener('click', function () { show(tab.dataset.tab); });
  });

  var search = document.getElementById('search');
  var matches = document.getElementById('matches');

  function filter() {
    var query = search.value.trim().toLowerCase();
    var total = 0;

    panes.forEach(function (pane) {
      var cards = Array.prototype.slice.call(pane.querySelectorAll('.card'));
      var shown = 0;
      cards.forEach(function (card) {
        var hit = !query || card.dataset.search.indexOf(query) !== -1;
        card.hidden = !hit;
        if (hit) { shown++; }
      });
      Array.prototype.slice.call(pane.querySelectorAll('.group')).forEach(function (group) {
        group.hidden = !group.querySelector('.card:not([hidden])');
      });

      var nothing = pane.querySelector('.nothing');
      if (nothing) { nothing.hidden = shown > 0 || cards.length === 0; }

      var badge = document.querySelector('.tab[data-tab="' + pane.dataset.pane + '"] .tab-count');
      if (badge) {
        badge.textContent = query ? shown + ' / ' + badge.dataset.total : badge.dataset.total;
      }
      total += shown;
    });

    matches.textContent = query ? total + ' matching' : '';
  }
  if (search) { search.addEventListener('input', filter); }

  var viewer = document.getElementById('viewer');
  var body = document.getElementById('viewer-body');
  var title = document.getElementById('viewer-title');
  var open = document.getElementById('viewer-open');

  function openCard(card) {
    var chart = card.dataset.chart;
    var image = card.dataset.image;
    title.textContent = card.dataset.title;
    open.href = chart || image || '#';
    body.innerHTML = '';

    if (chart) {
      var frame = document.createElement('iframe');
      frame.src = chart;
      frame.title = card.dataset.title;
      body.appendChild(frame);
    } else if (image) {
      var picture = document.createElement('img');
      picture.src = image;
      picture.alt = card.dataset.title;
      picture.className = 'fit';
      picture.addEventListener('click', function () {
        picture.classList.toggle('fit');
      });
      body.appendChild(picture);
    }

    if (viewer.showModal) { viewer.showModal(); } else { viewer.setAttribute('open', ''); }
  }

  Array.prototype.slice.call(document.querySelectorAll('.thumb')).forEach(function (button) {
    button.addEventListener('click', function () {
      openCard(button.parentNode);
    });
  });

  function close() {
    if (viewer.close) { viewer.close(); } else { viewer.removeAttribute('open'); }
    body.innerHTML = '';
  }
  document.getElementById('viewer-close').addEventListener('click', close);
  viewer.addEventListener('close', function () { body.innerHTML = ''; });
  viewer.addEventListener('click', function (event) {
    if (event.target === viewer) { close(); }
  });
})();
</script>
"#;

/// The whole appearance of the report, inline.
///
/// Inline because a report is copied, mailed and opened from a memory stick,
/// and a stylesheet left behind turns it into an unreadable outline. The font
/// is whatever the machine has: naming one that has to be downloaded would
/// break the same way.
const STYLE: &str = r#"<style>
:root {
  --ink: #14161a;
  --muted: #5b6470;
  --line: #dfe3e8;
  --paper: #ffffff;
  --panel: #f6f7f9;
  --accent: #1f5fa8;
}
* { box-sizing: border-box; }
body {
  margin: 0 auto;
  max-width: 1400px;
  padding: 2.5rem 1.5rem 5rem;
  font: 16px/1.55 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  color: var(--ink);
  background: var(--paper);
}
h1 { font-size: 1.9rem; margin: 0 0 1rem; letter-spacing: -0.01em; }
h2 { font-size: 1.05rem; margin: 2rem 0 0.8rem; padding-bottom: 0.35rem;
     border-bottom: 1px solid var(--line); }
code { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
       font-size: 0.9em; background: var(--panel); padding: 0.15em 0.4em;
       border-radius: 3px; }
header { border-bottom: 2px solid var(--ink); padding-bottom: 1.2rem; }
header p { margin: 0.35rem 0; color: var(--muted); }
header .label { display: inline-block; min-width: 9rem; color: var(--ink);
                font-weight: 600; }
header .summary { margin-top: 0.9rem; color: var(--ink); }

.controls { position: sticky; top: 0; z-index: 5; display: flex; flex-wrap: wrap;
            gap: 0.75rem; align-items: center; justify-content: space-between;
            padding: 0.7rem 0; margin-bottom: 0.5rem;
            background: var(--paper); border-bottom: 1px solid var(--line); }
.tabs { display: flex; flex-wrap: wrap; gap: 0.35rem; }
.tab { font: inherit; font-size: 0.92rem; color: var(--muted); cursor: pointer;
       padding: 0.4rem 0.8rem; border: 1px solid transparent; border-radius: 999px;
       background: transparent; }
.tab:hover { background: var(--panel); }
.tab[aria-selected="true"] { color: var(--paper); background: var(--ink);
                             border-color: var(--ink); }
.tab-count { font-size: 0.8em; opacity: 0.7; margin-left: 0.35rem; }
.find { display: flex; align-items: center; gap: 0.6rem; }
#search { font: inherit; font-size: 0.92rem; width: 22rem; max-width: 45vw;
          padding: 0.4rem 0.7rem; border: 1px solid var(--line);
          border-radius: 999px; background: var(--panel); color: var(--ink); }
#search:focus { outline: 2px solid var(--accent); outline-offset: 1px; }
#matches { font-size: 0.85rem; color: var(--muted); }

.cards { display: grid; gap: 1.1rem;
         grid-template-columns: repeat(auto-fill, minmax(330px, 1fr)); }
.card { margin: 0; border: 1px solid var(--line); border-radius: 6px;
        background: var(--panel); overflow: hidden; }
.thumb { display: block; width: 100%; padding: 0; border: 0; cursor: zoom-in;
         background: var(--paper); }
.thumb img { display: block; width: 100%; height: auto;
             aspect-ratio: var(--ratio, 16 / 10); object-fit: contain; }
.no-thumb { display: flex; align-items: center; justify-content: center;
            aspect-ratio: var(--ratio, 16 / 10); color: var(--muted);
            font-size: 0.85rem; }
figcaption { display: flex; flex-wrap: wrap; gap: 0.5rem;
             justify-content: space-between; align-items: baseline;
             font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
             font-size: 0.82rem; padding: 0.55rem 0.8rem;
             border-top: 1px solid var(--line); background: var(--paper); }
figcaption .dir { color: var(--muted); font-size: 0.9em; }
.links { margin: 0; padding: 0.45rem 0.8rem; font-size: 0.82rem;
         border-top: 1px solid var(--line); background: var(--paper); }
.links a, nav a { color: var(--accent); text-decoration: none; }
.links a:hover { text-decoration: underline; }
.nothing, .empty { padding: 2rem 0; color: var(--muted); }

#viewer { width: 94vw; max-width: 94vw; height: 92vh; padding: 0; border: 0;
          border-radius: 8px; background: var(--paper); color: var(--ink); }
#viewer::backdrop { background: rgba(10, 12, 16, 0.6); }
.viewer-bar { display: flex; align-items: center; justify-content: space-between;
              gap: 1rem; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line);
              font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
              font-size: 0.9rem; }
.viewer-actions { display: flex; align-items: center; gap: 0.9rem; }
.viewer-actions a { color: var(--accent); text-decoration: none; font-size: 0.85rem; }
#viewer-close { font: inherit; font-size: 0.85rem; cursor: pointer;
                padding: 0.3rem 0.8rem; border: 1px solid var(--line);
                border-radius: 4px; background: var(--panel); color: var(--ink); }
#viewer-body { height: calc(92vh - 3.1rem); overflow: auto; }
#viewer-body iframe { width: 100%; height: 100%; border: 0; }
#viewer-body img { display: block; cursor: zoom-in; }
#viewer-body img.fit { width: 100%; height: 100%; object-fit: contain;
                       cursor: zoom-in; }

.tree, .tree ul { list-style: none; margin: 0; padding-left: 1.1rem; }
.tree { padding-left: 0; font-family: ui-monospace, SFMono-Regular, Menlo,
        Consolas, monospace; font-size: 0.87rem; }
.tree summary { cursor: pointer; font-weight: 600; }
.tree .file { color: var(--muted); }
.tree .size { color: var(--muted); font-size: 0.85em; }

@media print {
  body { max-width: none; padding: 0; }
  .controls, .links { display: none; }
  .card { break-inside: avoid; }
  .pane[hidden] { display: block !important; }
}
</style>
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::tree::{Figure, Gallery};
    use std::path::PathBuf;

    fn figure(stem: &str, image: Option<&str>, chart: Option<&str>) -> Figure {
        Figure {
            stem: stem.to_string(),
            image: image.map(PathBuf::from),
            chart: chart.map(PathBuf::from),
        }
    }

    fn output() -> Output {
        Output {
            galleries: vec![Gallery {
                directory: PathBuf::from("Assortativity"),
                figures: vec![figure(
                    "abundance",
                    Some("Assortativity/abundance.png"),
                    Some("Assortativity/abundance.html"),
                )],
            }],
            tree: vec![Entry::Directory {
                name: "Assortativity".to_string(),
                children: vec![Entry::File {
                    name: "abundance.png".to_string(),
                    bytes: 2048,
                }],
            }],
            files: 1,
            bytes: 2048,
        }
    }

    /// A cohort's worth of output: two analyses, cohort figures and per-sample
    /// ones.
    fn full_output() -> Output {
        Output {
            galleries: vec![
                Gallery {
                    directory: PathBuf::from("Tysserand_Network"),
                    figures: vec![
                        figure(
                            "net_1-8",
                            Some("Tysserand_Network/net_1-8.png"),
                            Some("Tysserand_Network/net_1-8.html"),
                        ),
                        figure(
                            "net_2-6",
                            Some("Tysserand_Network/net_2-6.png"),
                            Some("Tysserand_Network/net_2-6.html"),
                        ),
                    ],
                },
                Gallery {
                    directory: PathBuf::from("Assortativity"),
                    figures: vec![figure(
                        "abundance",
                        Some("Assortativity/abundance.png"),
                        Some("Assortativity/abundance.html"),
                    )],
                },
                Gallery {
                    directory: PathBuf::from("Assortativity/assort_files"),
                    figures: vec![figure(
                        "heatmap_zscore_1-8",
                        Some("Assortativity/assort_files/heatmap_zscore_1-8.png"),
                        Some("Assortativity/assort_files/heatmap_zscore_1-8.html"),
                    )],
                },
                Gallery {
                    directory: PathBuf::from("Niche_Analysis/Aggregation/run"),
                    figures: vec![figure(
                        "Niches_Histogram",
                        Some("Niche_Analysis/Aggregation/run/Niches_Histogram.png"),
                        None,
                    )],
                },
            ],
            tree: vec![Entry::File {
                name: "net_stat.csv".to_string(),
                bytes: 4096,
            }],
            files: 6,
            bytes: 4096,
        }
    }

    fn page(output: &Output) -> String {
        render(&Page {
            working_dir: Path::new("/runs/cohort"),
            generated: "2026-08-18 16:05 UTC",
            output,
        })
    }

    /// The one number in the report a reader compares against `du`.
    #[test]
    fn a_size_is_written_the_way_a_person_reads_one() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1_000), "1.0 kB");
        assert_eq!(human_bytes(2_048), "2.0 kB");
        assert_eq!(human_bytes(1_500_000), "1.5 MB");
        assert_eq!(human_bytes(78_000_000), "78.0 MB");
        assert_eq!(human_bytes(3_200_000_000), "3.2 GB");
    }

    #[test]
    fn text_that_looks_like_markup_is_escaped() {
        assert_eq!(escape("a<b>&c"), "a&lt;b&gt;&amp;c");
        assert_eq!(escape("say \"this\""), "say &quot;this&quot;");
        assert_eq!(escape("it's"), "it&#39;s");
    }

    /// A Windows path is spelled with backslashes, which are not path
    /// separators in a URL: `src="Assortativity\abundance.html"` asks for one
    /// file with a strange name, and finds nothing.
    #[test]
    fn a_windows_path_becomes_a_url_with_forward_slashes() {
        assert_eq!(
            href(Path::new(
                r"Assortativity\assort_files\heatmap_zscore_1-8.html"
            )),
            "Assortativity/assort_files/heatmap_zscore_1-8.html"
        );
    }

    /// This is not hypothetical: `normalize: all` writes
    /// `Niches_Aggregated_Composition_niche&obs.png`. An ampersand left raw in
    /// an attribute is the start of an entity, and the link breaks.
    #[test]
    fn an_ampersand_in_a_file_name_survives_the_round_trip() {
        let url = href(Path::new("run/Niches_Aggregated_Composition_niche&obs.png"));
        let attribute = escape(&url);

        assert!(
            attribute.contains("niche&amp;obs"),
            "the ampersand was left raw: {attribute}"
        );
        assert!(
            !attribute.contains("%26"),
            "an ampersand is legal in a URL path and must not be encoded: {attribute}"
        );
    }

    /// A space is not legal in a URL, and a saving directory named by a user
    /// may well have one.
    #[test]
    fn a_space_and_a_hash_are_percent_encoded() {
        assert_eq!(href(Path::new("my run/net_1.png")), "my%20run/net_1.png");
        assert_eq!(
            href(Path::new("run#2/net_1.png")),
            "run%232/net_1.png",
            "an unencoded hash would be read as a fragment and the path lost"
        );
        assert_eq!(href(Path::new("100%/net_1.png")), "100%25/net_1.png");
    }

    #[test]
    fn the_page_is_a_complete_document() {
        let html = page(&output());
        assert!(html.starts_with("<!doctype html>"), "no doctype");
        assert!(html.contains("<html lang=\"en\">"));
        assert!(html.contains("charset=\"utf-8\""));
        assert!(html.trim_end().ends_with("</html>"));
    }

    /// A lab machine with no network must render the report identically.
    #[test]
    fn nothing_is_fetched_from_a_network() {
        let html = page(&output());
        for scheme in ["http://", "https://", "//cdn", "@import"] {
            assert!(
                !html.contains(scheme),
                "the report reaches for {scheme}, which a machine offline cannot"
            );
        }
    }

    #[test]
    fn the_header_says_which_directory_and_when() {
        let html = page(&output());
        assert!(html.contains("/runs/cohort"));
        assert!(html.contains("2026-08-18 16:05 UTC"));
    }

    /// One tab per analysis, named, and each with the count of what is in it.
    #[test]
    fn each_analysis_gets_a_tab() {
        let html = page(&full_output());

        for name in ["Networks", "Assortativity", "Niches"] {
            assert!(html.contains(name), "no {name} tab");
        }
        assert!(
            html.contains("data-tab=\"networks\""),
            "the tabs are not switchable"
        );
        assert!(
            html.contains("id=\"pane-networks\""),
            "a tab has no pane to show"
        );
    }

    /// The listing is a tab of its own rather than a slab at the bottom of
    /// every other one.
    #[test]
    fn the_listing_has_a_tab_of_its_own() {
        let html = page(&full_output());
        assert!(html.contains("data-tab=\"files\""));
        assert!(html.contains("id=\"pane-files\""));
    }

    /// Twenty-four charts of four hundred kilobytes each is ten megabytes.
    /// Nothing heavier than a thumbnail is fetched until a figure is opened.
    #[test]
    fn nothing_heavy_is_loaded_before_a_figure_is_opened() {
        let html = page(&full_output());

        assert!(
            !html.contains("<iframe"),
            "a chart is loaded before anyone asked for it"
        );
        assert!(
            html.contains("data-chart=\"Assortativity/abundance.html\""),
            "the chart is not recorded for the viewer to open"
        );
        assert!(
            html.contains("loading=\"lazy\""),
            "the thumbnails are not deferred"
        );
    }

    /// A thumbnail is what makes the page readable at a glance: the whole
    /// figure, small, several to a row — and the zoom is one click away.
    #[test]
    fn a_figure_is_shown_as_a_thumbnail_that_opens() {
        let html = page(&full_output());

        assert!(html.contains("class=\"thumb\""), "no thumbnail");
        assert!(
            html.contains("src=\"Assortativity/abundance.png\""),
            "the thumbnail is not the image"
        );
        assert!(
            html.contains("id=\"viewer\""),
            "nothing to open the figure into"
        );
    }

    /// The cohort first, then the patients: what happened, then who it
    /// happened to.
    #[test]
    fn a_pane_puts_the_cohort_before_the_patients() {
        let html = page(&full_output());
        let pane = &html[html.find("id=\"pane-assortativity\"").unwrap()..];

        let cohort = pane.find("The cohort").expect("no cohort heading");
        let patient = pane.find("Patient 1").expect("no patient heading");
        assert!(cohort < patient, "the patients come before the cohort");
    }

    /// The search box, and what every card answers it with.
    #[test]
    fn every_card_carries_what_the_search_matches() {
        let html = page(&full_output());

        assert!(html.contains("id=\"search\""), "no search box");
        assert!(html.contains("data-search"), "the cards cannot be searched");

        // A network of patient 1, sample 8 answers to either half and to both.
        let card = html
            .split("<figure")
            .find(|block| block.contains("net_1-8"))
            .expect("no card for net_1-8");
        let key = card
            .split("data-search=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_default();
        assert!(key.contains("1-8"), "the pair is not searchable: {key}");
        assert!(
            key.contains("net_1-8"),
            "the file name is not searchable: {key}"
        );
    }

    /// The script that switches tabs and filters has to be in the file, like
    /// everything else: a report is opened from a memory stick as often as not.
    #[test]
    fn the_behaviour_travels_with_the_page() {
        let html = page(&full_output());

        assert!(html.contains("<script>"), "the page does nothing");
        assert!(
            !html.contains("<script src="),
            "the script is fetched from somewhere"
        );
    }

    /// The image is what a reader pastes into a slide, so it has to be one
    /// click away — and it is also the fallback when there is no chart.
    #[test]
    fn the_image_is_linked_beside_the_chart() {
        let html = page(&output());
        assert!(html.contains("href=\"Assortativity/abundance.png\""));
    }

    #[test]
    fn a_figure_without_a_chart_is_still_shown() {
        let output = Output {
            galleries: vec![Gallery {
                directory: PathBuf::from("Tysserand_Network"),
                figures: vec![figure("net_1", Some("Tysserand_Network/net_1.png"), None)],
            }],
            ..output()
        };
        let html = page(&output);

        assert!(html.contains("<img"), "the image was not shown");
        assert!(html.contains("src=\"Tysserand_Network/net_1.png\""));
    }

    /// A directory tidied by hand, or written by an older version: the chart is
    /// there and the image is not. The card has no thumbnail to show, and must
    /// still offer the figure rather than disappear.
    #[test]
    fn a_figure_without_an_image_is_still_shown() {
        let output = Output {
            galleries: vec![Gallery {
                directory: PathBuf::from("Tysserand_Network"),
                figures: vec![figure("net_1", None, Some("Tysserand_Network/net_1.html"))],
            }],
            ..output()
        };
        let html = page(&output);

        assert!(html.contains("data-chart=\"Tysserand_Network/net_1.html\""));
        assert!(html.contains("net_1"), "the figure vanished with its image");
    }

    /// The tree is the other half of the request: what is in the directory,
    /// figures and tables alike.
    #[test]
    fn the_listing_is_in_the_page_with_its_sizes() {
        let html = page(&output());
        assert!(html.contains("abundance.png"));
        assert!(html.contains("2.0 kB"), "the size is not shown: {html}");
    }

    /// A working directory with nothing in it yet is the most likely first use:
    /// somebody presses the button before running an analysis.
    #[test]
    fn an_empty_output_produces_a_page_that_says_so() {
        let html = page(&Output::default());
        assert!(html.starts_with("<!doctype html>"));
        assert!(
            html.to_lowercase().contains("no figure"),
            "an empty report does not explain itself"
        );
    }

    /// The count is what a reader checks first: did everything get drawn.
    #[test]
    fn the_summary_counts_the_figures_and_the_files() {
        let html = page(&output());
        assert!(html.contains("1 figure"), "{html}");
    }

    /// A directory named by a user can contain anything; nothing it contains
    /// may become markup.
    #[test]
    fn a_hostile_file_name_cannot_inject_markup() {
        let output = Output {
            galleries: vec![Gallery {
                directory: PathBuf::from("<script>alert(1)</script>"),
                figures: vec![figure("<img onerror=x>", Some("a.png"), None)],
            }],
            tree: vec![Entry::File {
                name: "<script>bad</script>".to_string(),
                bytes: 1,
            }],
            files: 1,
            bytes: 1,
        };
        let html = page(&output);

        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(!html.contains("<script>bad</script>"));
        assert!(!html.contains("<img onerror=x>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
