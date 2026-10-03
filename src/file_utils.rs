#[cfg(unix)]
use rustix::{fs::Mode, process::umask};
use std::{
    io,
    path::{Component, Path, PathBuf},
};

/// Guarantee that the path is relative and cannot traverse back to parent directories
/// and optionally prevent traversing hidden directories.
///
/// See the unit tests tests::test_sanitize_path* for examples
pub fn sanitize_path(path: impl AsRef<Path>, traverse_hidden: bool) -> Option<PathBuf> {
    let mut buf = PathBuf::new();
    let mut traversed_parent = false;

    for comp in path.as_ref().components() {
        match comp {
            Component::Normal(name) => buf.push(name),
            Component::ParentDir => {
                traversed_parent = true;
                buf.pop();
            }
            _ => (),
        }
    }

    // Disallow `..` components that resolve to or above the served root.
    if traversed_parent && buf.as_os_str().is_empty() {
        return None;
    }

    // Unless hidden traversal is explicitly allowed, reject any path that still
    // contains a hidden component (a name starting with a dot) after resolution.
    if !traverse_hidden {
        for comp in buf.components() {
            match comp {
                Component::Normal(name) if name.to_str()?.starts_with('.') => return None,
                _ => (),
            }
        }
    }

    Some(buf)
}

/// Checks if any segment of the path is a symlink.
///
/// This function fails if [`std::fs::symlink_metadata`] fails, which usually
/// means user has no permission to access the path.
pub fn contains_symlink(path: impl AsRef<Path>) -> io::Result<bool> {
    let contains_symlink = path
        .as_ref()
        .ancestors()
        // On Windows, `\\?\` won't exist even though it's the root, but there's no need to check it
        // So we filter it out
        .filter(|p| p.exists())
        .map(|p| p.symlink_metadata())
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .any(|p| p.file_type().is_symlink());

    Ok(contains_symlink)
}

/// Get default file creation permissions by umask
#[cfg(unix)]
pub fn get_default_filemode() -> u16 {
    let old = umask(Mode::all());
    umask(old);
    let mode = 0o666 & (!old).as_raw_mode();
    mode as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    #[rstest]
    #[case("/foo", "foo")]
    #[case("////foo", "foo")]
    #[case("C:/foo", if cfg!(windows) { "foo" } else { "C:/foo" })]
    #[case("../foo", "foo")]
    #[case("../foo/../bar/abc", "bar/abc")]
    fn test_sanitize_path(#[case] input: &str, #[case] output: &str) {
        assert_eq!(
            sanitize_path(Path::new(input), true).unwrap(),
            Path::new(output)
        );
        assert_eq!(
            sanitize_path(Path::new(input), false).unwrap(),
            Path::new(output)
        );
    }

    #[rstest]
    #[case(".foo")]
    #[case("/.foo")]
    #[case("foo/.bar/foo")]
    fn test_sanitize_path_no_hidden_files(#[case] input: &str) {
        assert_eq!(sanitize_path(Path::new(input), false), None);
    }
}
