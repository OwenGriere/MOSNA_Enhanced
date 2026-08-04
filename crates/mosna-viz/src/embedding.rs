//! The clustered projection — port of `plotting.py::plot_clusters`.

use std::path::Path;

use plotters::prelude::*;

use crate::canvas::{colour, figure, label_style};
use crate::colormap::make_cluster_cmap;
use crate::theme::Theme;

/// Figure size in inches, from `plot_clusters(figsize=(10, 10))`.
const SIZE_INCHES: f64 = 10.0;

/// Draw the two-dimensional embedding, coloured by cluster.
///
/// Each cluster's id is written at its centroid, as `show_id=True` does in the
/// Python — that is what lets a reader match a blob in the projection to a
/// column of the composition heatmap.
///
/// A projection with fewer than two dimensions cannot be scattered in a plane;
/// nothing is drawn rather than something misleading.
pub fn draw(
    theme: &Theme,
    embedding: &[f64],
    n_components: usize,
    labels: &[u32],
    parameters: &str,
    save_dir: &Path,
) -> anyhow::Result<()> {
    if n_components < 2 || labels.is_empty() {
        return Ok(());
    }

    let path = save_dir.join(format!("cluster_labels{parameters}.png"));

    let points: Vec<(f64, f64)> = (0..labels.len())
        .filter_map(|row| {
            let base = row * n_components;
            Some((*embedding.get(base)?, *embedding.get(base + 1)?))
        })
        .collect();
    if points.is_empty() {
        return Ok(());
    }

    let mut clusters: Vec<u32> = labels.to_vec();
    clusters.sort_unstable();
    clusters.dedup();
    let palette = make_cluster_cmap(clusters.len());

    figure(&path, theme, SIZE_INCHES, SIZE_INCHES, |root| {
        let (mut min_x, mut max_x) = (f64::INFINITY, f64::NEG_INFINITY);
        let (mut min_y, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY);
        for (x, y) in &points {
            min_x = min_x.min(*x);
            max_x = max_x.max(*x);
            min_y = min_y.min(*y);
            max_y = max_y.max(*y);
        }
        let pad = ((max_x - min_x).max(max_y - min_y) * 0.05).max(1e-6);

        let mut chart = ChartBuilder::on(root)
            .margin(theme.stroke(8.0))
            .build_cartesian_2d(min_x - pad..max_x + pad, min_y - pad..max_y + pad)
            .map_err(|e| anyhow::anyhow!("cannot build the embedding axes: {e}"))?;

        let radius = theme.stroke(2.0) as i32;
        for (index, cluster) in clusters.iter().enumerate() {
            let fill = colour(palette[index]).filled();
            chart
                .draw_series(
                    points
                        .iter()
                        .zip(labels)
                        .filter(|(_, label)| *label == cluster)
                        .map(|((x, y), _)| Circle::new((*x, *y), radius, fill)),
                )
                .map_err(|e| anyhow::anyhow!("cannot draw a cluster: {e}"))?;
        }

        // The cluster id at each centroid.
        let style = label_style(theme, 12.0);
        for cluster in &clusters {
            let members: Vec<&(f64, f64)> = points
                .iter()
                .zip(labels)
                .filter(|(_, label)| *label == cluster)
                .map(|(point, _)| point)
                .collect();
            if members.is_empty() {
                continue;
            }
            let count = members.len() as f64;
            let centre = (
                members.iter().map(|(x, _)| x).sum::<f64>() / count,
                members.iter().map(|(_, y)| y).sum::<f64>() / count,
            );
            chart
                .draw_series(std::iter::once(Text::new(
                    cluster.to_string(),
                    centre,
                    style.clone(),
                )))
                .map_err(|e| anyhow::anyhow!("cannot label a cluster: {e}"))?;
        }

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Vec<f64>, Vec<u32>) {
        let mut embedding = Vec::new();
        let mut labels = Vec::new();
        for cluster in 0..3u32 {
            for i in 0..10 {
                embedding.push(cluster as f64 * 10.0 + i as f64 * 0.1);
                embedding.push(i as f64 * 0.1);
                labels.push(cluster);
            }
        }
        (embedding, labels)
    }

    #[test]
    fn the_parameters_reach_the_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let (embedding, labels) = fixture();
        draw(
            &Theme { dpi: 20.0 },
            &embedding,
            2,
            &labels,
            "_metric-cosine",
            dir.path(),
        )
        .unwrap();
        assert!(dir
            .path()
            .join("cluster_labels_metric-cosine.png")
            .is_file());
    }

    #[test]
    fn a_one_dimensional_projection_draws_nothing() {
        let dir = tempfile::tempdir().unwrap();
        draw(
            &Theme { dpi: 20.0 },
            &[1.0, 2.0],
            1,
            &[0, 1],
            "",
            dir.path(),
        )
        .unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn an_empty_labelling_draws_nothing() {
        let dir = tempfile::tempdir().unwrap();
        draw(&Theme { dpi: 20.0 }, &[], 2, &[], "", dir.path()).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn coincident_points_do_not_collapse_the_axes() {
        let dir = tempfile::tempdir().unwrap();
        draw(
            &Theme { dpi: 20.0 },
            &[1.0, 1.0, 1.0, 1.0],
            2,
            &[0, 1],
            "",
            dir.path(),
        )
        .unwrap();
        assert!(dir.path().join("cluster_labels.png").is_file());
    }
}
