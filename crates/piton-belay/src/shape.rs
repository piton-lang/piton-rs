//! Shape mapping.
//!
//! The shape tree approximately mirrors the application source tree, and an
//! instruction attaches to the closest *existing* implementation scope. When the
//! corresponding directory under `codeRoot` does not exist, placement walks
//! upward until it finds one, stopping at `codeRoot` itself.
//!
//! Fallback changes where scoped guidance is placed. It does not change the
//! original relative location, which the compiled shape-reference tree
//! preserves.

use std::path::{Path, PathBuf};

use piton_compile::module::normalize;

/// Where an instruction's guidance goes, and where its shape reference goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// Directory beneath `codeRoot` that the scoped guidance file lands in.
    pub scope: PathBuf,
    /// The instruction's path relative to `shapeRoot`, preserved for the
    /// reference tree.
    pub relative: PathBuf,
    /// True when the exact corresponding directory did not exist and placement
    /// walked upward.
    pub fell_back: bool,
}

/// Resolves the scope an instruction attaches to.
///
/// `exists` decides whether a candidate directory is present; taking it as a
/// parameter keeps the rule testable without a filesystem.
pub fn place(
    source: &Path,
    shape_root: &Path,
    code_root: &Path,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<Placement> {
    let relative = source.strip_prefix(shape_root).ok()?.to_path_buf();
    let mut directory = relative.parent().unwrap_or(Path::new("")).to_path_buf();

    let mut fell_back = false;
    loop {
        let candidate = normalize(&code_root.join(&directory));
        if exists(&candidate) {
            return Some(Placement {
                scope: candidate,
                relative,
                fell_back,
            });
        }
        if directory.as_os_str().is_empty() {
            // Fallback stops at codeRoot; if even that is missing there is
            // nowhere to attach.
            return None;
        }
        fell_back = true;
        directory = directory.parent().unwrap_or(Path::new("")).to_path_buf();
    }
}

/// True when `source` lives beneath `shape_root`.
pub fn is_shape_source(source: &Path, shape_root: &Path) -> bool {
    source.strip_prefix(shape_root).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn existing(paths: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |candidate: &Path| paths.iter().any(|p| Path::new(p) == candidate)
    }

    #[test]
    fn a_matching_directory_is_used_directly() {
        let placement = place(
            Path::new("spec/shape/components/button/Button.pi"),
            Path::new("spec/shape"),
            Path::new("src"),
            &existing(&["src/components/button"]),
        )
        .expect("placement");
        assert_eq!(placement.scope, Path::new("src/components/button"));
        assert_eq!(placement.relative, Path::new("components/button/Button.pi"));
        assert!(!placement.fell_back);
    }

    #[test]
    fn two_sources_in_one_directory_share_a_scope() {
        let exists = existing(&["src/components/input"]);
        let first = place(
            Path::new("spec/shape/components/input/Input.pi"),
            Path::new("spec/shape"),
            Path::new("src"),
            &exists,
        )
        .expect("first");
        let second = place(
            Path::new("spec/shape/components/input/InputDesign.pi"),
            Path::new("spec/shape"),
            Path::new("src"),
            &exists,
        )
        .expect("second");
        assert_eq!(first.scope, second.scope);
    }

    #[test]
    fn a_missing_directory_walks_up_to_the_nearest_existing_one() {
        let placement = place(
            Path::new("spec/shape/components/nonexistent/Component.pi"),
            Path::new("spec/shape"),
            Path::new("src"),
            &existing(&["src/components", "src"]),
        )
        .expect("placement");
        assert_eq!(placement.scope, Path::new("src/components"));
        assert!(placement.fell_back);
        assert_eq!(
            placement.relative,
            Path::new("components/nonexistent/Component.pi"),
            "the original location is preserved for the reference tree"
        );
    }

    #[test]
    fn fallback_stops_at_the_code_root() {
        let placement = place(
            Path::new("spec/shape/a/b/c/Thing.pi"),
            Path::new("spec/shape"),
            Path::new("src"),
            &existing(&["src"]),
        )
        .expect("placement");
        assert_eq!(placement.scope, Path::new("src"));
    }

    #[test]
    fn a_missing_code_root_has_nowhere_to_attach() {
        assert!(place(
            Path::new("spec/shape/a/Thing.pi"),
            Path::new("spec/shape"),
            Path::new("src"),
            &existing(&[]),
        )
        .is_none());
    }
}
