#!/usr/bin/env python3
"""
Update version numbers across all AgentFS components.

Usage:
    python scripts/update-version.py 0.1.0-pre.1
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import List


def parse_version(version: str) -> str:
    """Validate and normalize version string."""
    # Basic semver validation (supports pre-release versions)
    pattern = r'^\d+\.\d+\.\d+(-[a-zA-Z0-9.-]+)?$'
    if not re.match(pattern, version):
        raise ValueError(
            f"Invalid version format: {version}. "
            "Expected format: X.Y.Z or X.Y.Z-pre.N"
        )
    return version


def update_cargo_toml(file_path: Path, new_version: str) -> bool:
    """Update version in a Cargo.toml file."""
    try:
        content = file_path.read_text()

        # Find and replace the version line under [package]
        # Match version = "..." with proper spacing
        pattern = r'(^\[package\].*?^version\s*=\s*)"[^"]*"'
        replacement = rf'\1"{new_version}"'

        new_content = re.sub(
            pattern,
            replacement,
            content,
            count=1,
            flags=re.MULTILINE | re.DOTALL
        )

        if new_content == content:
            print(f"Warning: No version field found in {file_path}")
            return False

        file_path.write_text(new_content)
        print(f" Updated {file_path.relative_to(file_path.parents[3])}")
        return True

    except Exception as e:
        print(f"Error updating {file_path}: {e}", file=sys.stderr)
        return False


def update_package_json(file_path: Path, new_version: str) -> bool:
    """Update version in a package.json file."""
    try:
        with open(file_path, 'r') as f:
            data = json.load(f)

        if 'version' not in data:
            print(f"Warning: No version field found in {file_path}")
            return False

        data['version'] = new_version

        with open(file_path, 'w') as f:
            json.dump(data, f, indent=2)
            f.write('\n')  # Add trailing newline

        print(f" Updated {file_path.relative_to(file_path.parents[3])}")
        return True

    except Exception as e:
        print(f"Error updating {file_path}: {e}", file=sys.stderr)
        return False


def update_cargo_lock(crate_dir: Path) -> bool:
    """Update Cargo.lock by regenerating it in the crate directory."""
    try:
        # Use cargo generate-lockfile to force regeneration of Cargo.lock
        # This ensures path dependencies get their versions updated
        result = subprocess.run(
            ['cargo', 'generate-lockfile'],
            cwd=crate_dir,
            capture_output=True,
            text=True
        )

        if result.returncode == 0:
            return True
        else:
            print(f"Error updating Cargo.lock: {result.stderr}", file=sys.stderr)
            return False

    except Exception as e:
        print(f"Error updating Cargo.lock: {e}", file=sys.stderr)
        return False


def update_package_lock(package_dir: Path) -> bool:
    """Update package-lock.json by running npm install."""
    try:
        result = subprocess.run(
            ['npm', 'install'],
            cwd=package_dir,
            capture_output=True,
            text=True
        )

        if result.returncode == 0:
            return True
        else:
            print(f"Error updating package-lock.json: {result.stderr}", file=sys.stderr)
            return False

    except Exception as e:
        print(f"Error updating package-lock.json: {e}", file=sys.stderr)
        return False


def git_commit_and_tag(root: Path, version: str, components: list) -> bool:
    """Create a git commit and tag for the version update."""
    try:
        print("\nCreating git commit and tag...")

        # Stage only the files we modified
        files_to_add = []
        for component in components:
            # Add version file (Cargo.toml or package.json)
            files_to_add.append(str(component['version_file'].relative_to(root)))

            # Add lock file (Cargo.lock or package-lock.json)
            if 'Cargo.toml' in str(component['version_file']):
                lock_file = component['lock_dir'] / 'Cargo.lock'
            else:
                lock_file = component['lock_dir'] / 'package-lock.json'

            if lock_file.exists():
                files_to_add.append(str(lock_file.relative_to(root)))

        result = subprocess.run(
            ['git', 'add'] + files_to_add,
            cwd=root,
            capture_output=True,
            text=True
        )

        if result.returncode != 0:
            print(f"Error staging changes: {result.stderr}", file=sys.stderr)
            return False

        # Commit
        commit_message = f"AgentFS {version}"
        result = subprocess.run(
            ['git', 'commit', '-m', commit_message],
            cwd=root,
            capture_output=True,
            text=True
        )

        if result.returncode != 0:
            print(f"Error creating commit: {result.stderr}", file=sys.stderr)
            return False

        print(f"✓ Created commit: {commit_message}")

        # Create tag
        tag_name = f"v{version}"
        result = subprocess.run(
            ['git', 'tag', tag_name],
            cwd=root,
            capture_output=True,
            text=True
        )

        if result.returncode != 0:
            print(f"Error creating tag: {result.stderr}", file=sys.stderr)
            return False

        print(f"✓ Created tag: {tag_name}")

        return True

    except Exception as e:
        print(f"Error in git operations: {e}", file=sys.stderr)
        return False


def find_project_root() -> Path:
    """Find the project root directory."""
    current = Path(__file__).resolve().parent
    # Go up until we find the root (where cli, sdk, sandbox directories are)
    while current.parent != current:
        if (current / 'cli').exists() and (current / 'sdk').exists():
            return current
        current = current.parent
    raise RuntimeError("Could not find project root")


def main():
    parser = argparse.ArgumentParser(
        description='Update version numbers across all AgentFS components'
    )
    parser.add_argument(
        'version',
        help='Version number (e.g., 0.1.0 or 0.1.0-pre.1)'
    )
    parser.add_argument(
        '--dry-run',
        action='store_true',
        help='Show what would be updated without making changes'
    )

    args = parser.parse_args()

    try:
        version = parse_version(args.version)
    except ValueError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

    # Find project root
    try:
        root = find_project_root()
    except RuntimeError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

    print(f"Updating version to: {version}")
    print(f"Project root: {root}\n")

    # Define all components to update
    components = [
        # Rust crates
        {
            'version_file': root / 'cli' / 'Cargo.toml',
            'version_func': update_cargo_toml,
            'lock_dir': root / 'cli',
            'lock_func': update_cargo_lock,
            'name': 'cli'
        },
        {
            'version_file': root / 'sandbox' / 'Cargo.toml',
            'version_func': update_cargo_toml,
            'lock_dir': root / 'sandbox',
            'lock_func': update_cargo_lock,
            'name': 'sandbox'
        },
        {
            'version_file': root / 'sdk' / 'rust' / 'Cargo.toml',
            'version_func': update_cargo_toml,
            'lock_dir': root / 'sdk' / 'rust',
            'lock_func': update_cargo_lock,
            'name': 'sdk/rust'
        },
        # TypeScript SDK
        {
            'version_file': root / 'sdk' / 'typescript' / 'package.json',
            'version_func': update_package_json,
            'lock_dir': root / 'sdk' / 'typescript',
            'lock_func': update_package_lock,
            'name': 'sdk/typescript'
        },
    ]

    if args.dry_run:
        print("DRY RUN - No files will be modified\n")
        for component in components:
            if component['version_file'].exists():
                print(f"Would update: {component['version_file'].relative_to(root)}")
                print(f"  and lock file in {component['name']}/")
            else:
                print(f"Warning: File not found: {component['version_file'].relative_to(root)}")
        print("\nWould also:")
        print(f"  - Create git commit: 'AgentFS {version}'")
        print(f"  - Create git tag: 'v{version}'")
        sys.exit(0)

    # Update all components
    all_success = True

    for component in components:
        version_file = component['version_file']
        if not version_file.exists():
            print(f"Warning: File not found: {version_file.relative_to(root)}")
            all_success = False
            continue

        # Update version file
        if component['version_func'](version_file, version):
            # Update lock file
            if component['lock_func'](component['lock_dir']):
                print(f"✓ Updated {component['name']} lock file")
            else:
                all_success = False
        else:
            all_success = False

    if not all_success:
        print("\n❌ Some files failed to update", file=sys.stderr)
        sys.exit(1)

    # Update cli/Cargo.lock again to pick up new versions of workspace dependencies
    print("\nUpdating cli/Cargo.lock to pick up new dependency versions...")
    if update_cargo_lock(root / 'cli'):
        print("✓ Updated cli/Cargo.lock with new dependency versions")
    else:
        print("❌ Failed to update cli/Cargo.lock", file=sys.stderr)
        sys.exit(1)

    print("\n✅ All files and lock files updated successfully!")

    # Create git commit and tag
    if not git_commit_and_tag(root, version, components):
        print("\n❌ Git operations failed", file=sys.stderr)
        sys.exit(1)

    print("\n✅ Version update complete! Don't forget to push the commit and tag.")


if __name__ == '__main__':
    main()
