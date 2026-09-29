//! What the sensitivity screen offers when it opens on the shipped
//! configuration.
//!
//! The unit tests build forms from fixtures of their own. This one opens the
//! configuration that actually ships, because the screen's promise — that it
//! opens on the run the main interface would have started — is a promise about
//! real configurations, and the two ways it broke were both invisible to a
//! fixture written alongside the code.

use mosna_gui::model::sweep::{Sampling, SweepForm};

fn shipped() -> SweepForm {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../CONFIG/configuration.yaml"
    ))
    .expect("the shipped configuration");
    SweepForm::from_config(&mosna_config::RawConfig::from_yaml_str(&text).unwrap())
}

/// Every row opens on exactly the value in force, so the grid is the single run
/// the main interface would have produced.
#[test]
fn every_row_opens_on_the_value_the_configuration_holds() {
    let form = shipped();

    for axis in form.shared.iter().chain(
        form.clusterers
            .iter()
            .flat_map(|clusterer| clusterer.axes.iter()),
    ) {
        let points = axis.to_axis().points();
        assert_eq!(
            points.len(),
            1,
            "`{}` opens on {} values: {:?}",
            axis.key,
            points.len(),
            points.iter().map(|p| p.label()).collect::<Vec<_>>()
        );
        assert_eq!(axis.problem(), None, "`{}` opens on a problem", axis.key);
    }
}

/// And it is the configured value, not the row's own default. `n_clusters` is
/// 6 in the shipped file; the row used to open on the default of 15 because
/// refreshing the menu kept whatever tick it had been built with.
#[test]
fn a_whole_number_opens_on_the_configured_value() {
    let form = shipped();
    let expected = [
        ("n_neighbors", "20"),
        ("dim_clust", "2"),
        ("n_clusters", "6"),
        ("k_cluster", "20"),
    ];

    for (key, value) in expected {
        let axis = form
            .shared
            .iter()
            .chain(
                form.clusterers
                    .iter()
                    .flat_map(|clusterer| clusterer.axes.iter()),
            )
            .find(|axis| axis.key == key)
            .unwrap_or_else(|| panic!("`{key}` is not on the screen"));

        assert_eq!(
            axis.to_axis().points()[0].label(),
            value,
            "`{key}` did not open on the configured value"
        );
    }
}

/// A menu whose only entry is the value already in force is not a menu. A
/// configured `k_cluster` of 20 against a ceiling of 20 produced exactly that.
#[test]
fn every_whole_number_menu_offers_a_choice() {
    for axis in shipped().shared.iter().chain(
        shipped()
            .clusterers
            .iter()
            .flat_map(|clusterer| clusterer.axes.iter()),
    ) {
        if matches!(axis.sampling, Sampling::Integer) {
            assert!(
                axis.candidates().len() > 1,
                "`{}` offers only {:?}",
                axis.key,
                axis.candidates()
            );
        }
    }
}
