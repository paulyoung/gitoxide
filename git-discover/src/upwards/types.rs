use std::{borrow::Cow, env, path::PathBuf};

use bstr::{ByteSlice, ByteVec};

/// The error returned by [git_discover::upwards()][crate::upwards()].
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum Error {
    #[error("Could not obtain the current working directory")]
    CurrentDir(#[from] std::io::Error),
    #[error("Relative path \"{}\"tries to reach beyond root filesystem", directory.display())]
    InvalidInput { directory: PathBuf },
    #[error("Failed to access a directory, or path is not a directory: '{}'", .path.display())]
    InaccessibleDirectory { path: PathBuf },
    #[error("Could find a git repository in '{}' or in any of its parents", .path.display())]
    NoGitRepository { path: PathBuf },
    #[error("Could find a git repository in '{}' or in any of its parents within ceiling height of {}", .path.display(), .ceiling_height)]
    NoGitRepositoryWithinCeiling { path: PathBuf, ceiling_height: usize },
    #[error("Could find a git repository in '{}' or in any of its parents within device limits below '{}'", .path.display(), .limit.display())]
    NoGitRepositoryWithinFs { path: PathBuf, limit: PathBuf },
    #[error("None of the passed ceiling directories prefixed the git-dir candidate, making them ineffective.")]
    NoMatchingCeilingDir,
    #[error("Could find a trusted git repository in '{}' or in any of its parents, candidate at '{}' discarded", .path.display(), .candidate.display())]
    NoTrustedGitRepository {
        path: PathBuf,
        candidate: PathBuf,
        required: git_sec::Trust,
    },
    #[error("Could not determine trust level for path '{}'.", .path.display())]
    CheckTrust {
        path: PathBuf,
        #[source]
        err: std::io::Error,
    },
}

/// Options to help guide the [discovery][crate::upwards()] of repositories, along with their options
/// when instantiated.
pub struct Options<'a> {
    /// When discovering a repository, assure it has at least this trust level or ignore it otherwise.
    ///
    /// This defaults to [`Reduced`][git_sec::Trust::Reduced] as our default settings are geared towards avoiding abuse.
    /// Set it to `Full` to only see repositories that [are owned by the current user][git_sec::Trust::from_path_ownership()].
    pub required_trust: git_sec::Trust,
    /// When discovering a repository, ignore any repositories that are located in these directories or any of their parents.
    pub ceiling_dirs: Vec<PathBuf>,
    /// If true, and `ceiling_dirs` is not empty, we expect at least one ceiling directory to match or else there will be an error.
    pub match_ceiling_dir_or_error: bool,
    /// if `true` avoid crossing filesystem boundaries.
    /// Only supported on Unix-like systems.
    // TODO: test on Linux
    // TODO: Handle WASI once https://github.com/rust-lang/rust/issues/71213 is resolved
    pub cross_fs: bool,
    /// If set, the _current working directory_ (absolute path) to use when resolving relative paths. Note that
    /// that this is merely an optimization for those who discover a lot of repositories in the same process.
    ///
    /// If unset, the current working directory will be obtained automatically.
    pub current_dir: Option<&'a std::path::Path>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Options {
            required_trust: git_sec::Trust::Reduced,
            ceiling_dirs: vec![],
            match_ceiling_dir_or_error: true,
            cross_fs: false,
            current_dir: None,
        }
    }
}

impl Options<'_> {
    /// Loads discovery options overrides from the environment.
    ///
    /// The environment variables are:
    /// - `GIT_CEILING_DIRECTORIES` for `ceiling_dirs`
    ///
    /// Note that `GIT_DISCOVERY_ACROSS_FILESYSTEM` for `cross_fs` is **not** read,
    /// as it requires parsing of `git-config` style boolean values.
    // TODO: test
    pub fn apply_environment(mut self) -> Self {
        let name = "GIT_CEILING_DIRECTORIES";
        if let Some(ceiling_dirs) = env::var_os(name).and_then(|c| Vec::from_os_string(c).ok()) {
            self.ceiling_dirs = parse_ceiling_dirs(&ceiling_dirs);
        }
        self
    }
}

/// Parse a byte-string of `:`-separated paths into `Vec<PathBuf>`.
/// Non-absolute paths are discarded.
/// To match git, all paths are normalized, until an empty path is encountered.
pub(crate) fn parse_ceiling_dirs(ceiling_dirs: &[u8]) -> Vec<PathBuf> {
    let mut should_normalize = true;
    let mut result = Vec::new();
    for ceiling_dir in ceiling_dirs.split_str(":") {
        if ceiling_dir.is_empty() {
            should_normalize = false;
            continue;
        }

        // Paths that are invalid unicode can't be handled
        let mut dir = match ceiling_dir.to_path() {
            Ok(dir) => Cow::Borrowed(dir),
            Err(_) => continue,
        };

        // Only absolute paths are allowed
        if dir.is_relative() {
            continue;
        }

        if should_normalize {
            if let Ok(normalized) = git_path::realpath(&dir) {
                dir = Cow::Owned(normalized);
            }
        }
        result.push(dir.into_owned());
    }
    result
}

#[cfg(test)]
mod tests {

    #[test]
    #[cfg(unix)]
    fn parse_ceiling_dirs_from_environment_format() -> std::io::Result<()> {
        use std::{fs, os::unix::fs::symlink};

        use super::*;

        // Setup filesystem
        let dir = tempfile::tempdir().expect("success creating temp dir");
        let direct_path = dir.path().join("direct");
        let symlink_path = dir.path().join("symlink");
        fs::create_dir(&direct_path)?;
        symlink(&direct_path, &symlink_path)?;

        // Parse & build ceiling dirs string
        let symlink_str = symlink_path.to_str().expect("symlink path is valid utf8");
        let ceiling_dir_string = format!("{}:relative::{}", symlink_str, symlink_str);
        let ceiling_dirs = parse_ceiling_dirs(ceiling_dir_string.as_bytes());

        assert_eq!(ceiling_dirs.len(), 2, "Relative path is discarded");
        assert_eq!(
            ceiling_dirs[0],
            symlink_path.canonicalize().expect("symlink path exists"),
            "Symlinks are resolved"
        );
        assert_eq!(
            ceiling_dirs[1], symlink_path,
            "Symlink are not resolved after empty item"
        );

        dir.close()
    }
}
