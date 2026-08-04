//! Niche composition and the join between niches and phenotypes.
//!
//! Ports `mosna/niches.py` together with `package/core/NAS/find_all_pheno.py`
//! and `merge_niche_pheno.py`.

pub mod aggregate_cell_types;
pub mod composition;
pub mod find_all_phenotypes;
pub mod merge_niche_pheno;

pub use aggregate_cell_types::aggregate_cell_types;
pub use composition::{make_niches_composition, NicheComposition, Normalize};
pub use find_all_phenotypes::find_all_phenotypes;
pub use merge_niche_pheno::merge_niche_pheno;
