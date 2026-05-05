use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};

pub fn normalize_directory_for_storage(path: &Path) -> Result<PathBuf> {
    absolutize_path(&expand_home_directory(path))
}

pub fn normalize_directory_for_match(path: &Path) -> PathBuf {
    absolutize_path_best_effort(&expand_home_directory(path))
}

pub fn directories_match(left: &Path, right: &Path) -> bool {
    normalize_directory_for_match(left) == normalize_directory_for_match(right)
}

pub fn is_home_directory(path: &Path) -> bool {
    let Some(home_directory) = env::var_os("HOME").map(PathBuf::from) else {
        return false;
    };

    directories_match(path, &home_directory)
}

fn expand_home_directory(path: &Path) -> PathBuf {
    let Some(path_str) = path.to_str() else {
        return path.to_path_buf();
    };

    if path_str == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| path.to_path_buf());
    }

    let Some(remainder) = path_str.strip_prefix("~/") else {
        return path.to_path_buf();
    };

    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(remainder))
        .unwrap_or_else(|| path.to_path_buf())
}

fn absolutize_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    Ok(env::current_dir()
        .context("failed to determine current working directory")?
        .join(path))
}

fn absolutize_path_best_effort(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    env::current_dir()
        .map(|current_directory| current_directory.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}
