#!/usr/bin/env python3

from __future__ import annotations

from pathlib import PurePosixPath
import subprocess
import tomllib


NON_ARTIFACT_PATHS = {
    ".github",
    ".test-status.json",
    "AGENTS.md",
    "LICENSE",
    "README.md",
    "VISION.md",
    "docs",
    "scripts",
    "tests",
}


def cargo_version() -> str:
    with open("Cargo.toml", "rb") as cargo_toml:
        return tomllib.load(cargo_toml)["package"]["version"]


def git_stdout(*args: str) -> str:
    return subprocess.run(
        ["git", *args],
        check=True,
        text=True,
        capture_output=True,
    ).stdout.strip()


def tag_exists(tag: str) -> bool:
    return (
        subprocess.run(
            ["git", "rev-parse", "--verify", "--quiet", f"{tag}^{{commit}}"],
            check=False,
            text=True,
            capture_output=True,
        ).returncode
        == 0
    )


def changed_paths(base_commit: str, head_commit: str) -> list[str]:
    output = git_stdout("diff", "--name-only", f"{base_commit}..{head_commit}")
    return [line for line in output.splitlines() if line]


def is_non_artifact_path(path: str) -> bool:
    parts = PurePosixPath(path).parts

    if not parts:
        return False

    if path in NON_ARTIFACT_PATHS:
        return True

    return parts[0] in NON_ARTIFACT_PATHS


def main() -> int:
    version = cargo_version()
    tag = f"v{version}"

    if not tag_exists(tag):
        print(f"No existing release tag for {tag}; version bump check passes.")
        return 0

    tag_commit = git_stdout("rev-parse", f"{tag}^{{commit}}")
    head_commit = git_stdout("rev-parse", "HEAD")

    if tag_commit == head_commit:
        print(f"HEAD already matches release tag {tag}; version bump check passes.")
        return 0

    changed = changed_paths(tag_commit, head_commit)
    artifact_affecting = [path for path in changed if not is_non_artifact_path(path)]

    if not artifact_affecting:
        print(
            f"Changes since {tag} only touched non-artifact files; version bump check passes."
        )
        return 0

    print(
        f"Cargo.toml version {version} already has release tag {tag} at {tag_commit}, but HEAD is {head_commit}."
    )
    print(
        "The following changed paths affect the released binary and require a version bump:"
    )
    for path in artifact_affecting:
        print(f"- {path}")
    print("Bump package.version before pushing this change to main.")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
