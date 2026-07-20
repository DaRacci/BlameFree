//! FlattenedStruct trait — enables hierarchical structs to be
//! flattened to/from flat column-name→value maps for DB storage.

/// A trait for structs that can be flattened into a flat column map
/// and reconstructed from one.
///
/// Used by the `#[derive(FlattenedStruct)]` proc macro and consumed
/// internally by `FlattenedEnum` generated code.
pub trait FlattenedStruct: Sized {
    /// Return a list of (column_name, type_name) pairs for every leaf
    /// reachable from this struct, given the current path prefix.
    ///
    /// The column_name strings are `'static` via `Box::leak` — safe
    /// because they are a finite, fixed set derived from struct field
    /// names and attributes.
    fn leaf_fields(prefix: &str) -> Vec<(&'static str, &'static str)>;

    /// Flatten `self` into a list of (column_name, optional_value) pairs.
    fn flatten(&self, prefix: &str) -> Vec<(String, Option<String>)>;

    /// Reconstruct `self` from a flat column map.
    fn unflatten(
        prefix: &str,
        cols: &std::collections::HashMap<String, Option<String>>,
    ) -> Self;
}
