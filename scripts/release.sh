#!/usr/bin/env bash
set -euo pipefail

# buddha release helper
#
# What it does (when bump enabled):
# - Bumps workspace crate versions (buddha-core/buddha-cli/buddha-mcp)
# - Updates docs/buddha_system_prompt.txt header (version/date)
# - Updates README release examples version strings
# - Moves CHANGELOG [Unreleased] contents into a new version section
# - Runs cargo fmt + cargo test (unless disabled)
# - Commits, and optionally pushes / tags / creates GitHub release
#
# Usage:
#   scripts/release.sh <version> [--push] [--tag] [--release] [--auto-notes] [--dry-run] [--no-fmt] [--no-test] [--no-bump]
#   scripts/release.sh --patch [--push] ...
#
# Examples:
#   scripts/release.sh --patch --push
#   scripts/release.sh 0.6.3 --push
#   scripts/release.sh 0.6.3 --push --tag
#   scripts/release.sh 0.6.3 --push --tag --release --auto-notes

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

ver=""
patch_flag=0
push_flag=0
tag_flag=0
release_flag=0
auto_notes=0
dry_run=0
bump_flag=1
fmt_flag=1
test_flag=1

die() {
  echo "Error: $*" >&2
  exit 1
}

run() {
  if [[ "$dry_run" -eq 1 ]]; then
    echo "[dry-run] $*"
    return 0
  fi
  eval "$@"
}

usage() {
  cat <<EOF
Usage: scripts/release.sh <version>|--patch [options]

Options:
  --patch        Auto-increment patch version from buddha-mcp/Cargo.toml (e.g., 0.6.2 -> 0.6.3)
  --push         git push origin HEAD
  --tag          create annotated tag vX.Y.Z
  --release      create GitHub release via gh (requires --tag)
  --auto-notes   use GitHub auto-generated notes (only with --release)
  --dry-run      print actions; do not modify files or run git/gh
  --no-bump      skip all file modifications; only allow git/gh operations
  --no-fmt       skip cargo fmt
  --no-test      skip cargo tests

Examples:
  scripts/release.sh --patch --push
  scripts/release.sh 0.6.3 --push
  scripts/release.sh 0.6.3 --push --tag
  scripts/release.sh 0.6.3 --push --tag --release --auto-notes
EOF
}

while (( "$#" )); do
  case "$1" in
    --patch) patch_flag=1; shift ;;
    --push) push_flag=1; shift ;;
    --tag) tag_flag=1; shift ;;
    --release) release_flag=1; shift ;;
    --auto-notes) auto_notes=1; shift ;;
    --dry-run) dry_run=1; shift ;;
    --no-bump) bump_flag=0; shift ;;
    --no-fmt) fmt_flag=0; shift ;;
    --no-test) test_flag=0; shift ;;
    -h|--help) usage; exit 0 ;;
    *)
      if [[ -n "$ver" ]]; then
        die "unexpected extra arg: $1"
      fi
      ver="$1"
      shift
      ;;
  esac
done

if [[ "$patch_flag" -eq 1 && -n "$ver" ]]; then
  die "use either <version> or --patch, not both"
fi
if [[ "$patch_flag" -eq 0 && -z "$ver" ]]; then
  die "version is required (e.g., 0.6.3) or use --patch"
fi

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  die "not in a git repo"
fi
if [[ "$push_flag" -eq 1 ]]; then
  git remote get-url origin >/dev/null 2>&1 || die "origin remote not found"
fi
if [[ "$release_flag" -eq 1 && "$tag_flag" -ne 1 ]]; then
  die "--release requires --tag"
fi

read_current_version() {
  # Reads version from buddha-mcp/Cargo.toml
  perl -ne 'if (/^version[ \t]*=[ \t]*"(\d+\.\d+\.\d+)"[ \t]*$/) { print "$1\n"; exit }' buddha-mcp/Cargo.toml
}

inc_patch() {
  local v="$1"
  perl -e 'my ($v)=@ARGV; if ($v !~ /^(\d+)\.(\d+)\.(\d+)$/) { exit 2 } print "$1.$2.".($3+1);' "$v"
}

ver_no_v=""
if [[ "$patch_flag" -eq 1 ]]; then
  cur="$(read_current_version)"
  [[ -n "$cur" ]] || die "failed to read current version from buddha-mcp/Cargo.toml"
  ver_no_v="$(inc_patch "$cur")" || die "failed to increment patch from: $cur"
else
  ver_no_v="${ver#v}"
fi
[[ "$ver_no_v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "invalid version: $ver_no_v"

tag="v${ver_no_v}"
today="$(date +%F)"

update_versions() {
  local v="$1"
  local files=(buddha-core/Cargo.toml buddha-cli/Cargo.toml buddha-mcp/Cargo.toml)
  for f in "${files[@]}"; do
    if [[ "$dry_run" -eq 1 ]]; then
      echo "[dry-run] set $f version = \"$v\""
    else
      perl -pi -e 's/^version[ \t]*=[ \t]*"\d+\.\d+\.\d+"[ \t]*$/version = "'"$v"'"/' "$f"
    fi
  done
}

update_system_prompt_header() {
  local v="$1"
  local d="$2"
  local f="docs/buddha_system_prompt.txt"
  [[ -f "$f" ]] || die "missing $f (expected by buddha_system_prompt tool)"
  if [[ "$dry_run" -eq 1 ]]; then
    echo "[dry-run] update $f header -> Buddha MCP system prompt (v${v}, ${d})"
    return 0
  fi
  perl -pi -e 'if ($.==1) { $_="Buddha MCP system prompt (v'"$v"', '"$d"')\n" }' "$f"
}

update_readme_release_examples() {
  local v="$1"
  local files=(README.md README.ja.md README.zh-TW.md)
  for f in "${files[@]}"; do
    [[ -f "$f" ]] || continue
    if [[ "$dry_run" -eq 1 ]]; then
      echo "[dry-run] update $f release.sh examples -> $v"
    else
      perl -pi -e 's/(scripts\/release\.sh)\s+\d+\.\d+\.\d+/$1 '"$v"'/g' "$f"
    fi
  done
}

check_readme_has_system_prompt_tool() {
  local files=(README.md README.ja.md README.zh-TW.md)
  for f in "${files[@]}"; do
    [[ -f "$f" ]] || continue
    if ! rg -q "buddha_system_prompt" "$f"; then
      die "$f does not mention buddha_system_prompt (please add it under MCP Tools -> Core)"
    fi
  done
}

update_changelog_rollover() {
  local v="$1"
  local d="$2"
  local f="CHANGELOG.md"
  [[ -f "$f" ]] || die "missing $f"

  if [[ "$dry_run" -eq 1 ]]; then
    echo "[dry-run] rollover $f: move [Unreleased] body -> [$v] - $d"
    return 0
  fi

  BUDDHA_REL_VER="$v" BUDDHA_REL_DATE="$d" perl -0777 -i -pe '
    my $v = $ENV{BUDDHA_REL_VER};
    my $d = $ENV{BUDDHA_REL_DATE};
    my $bump = "- Version bumped: `buddha-core` $v, `buddha` $v, `buddha-mcp` $v.\n";

    my $needle = "## [Unreleased]\n";
    index($_, $needle) >= 0 or die "CHANGELOG missing ## [Unreleased]\n";

    my ($pre, $post) = split(/\Q$needle\E/, $_, 2);
    my ($body, $rest) = ("", "");
    if ($post =~ /\A(.*?)(\n## \[\d+\.\d+\.\d+\] - \d{4}-\d{2}-\d{2}.*)\z/s) {
      ($body, $rest) = ($1, $2);
    } else {
      ($body, $rest) = ($post, "");
    }

    $body =~ s/\A\n+//s;
    $body =~ s/\n+\z/\n/s;
    my $has_body = ($body =~ /\S/);

    my $new = "## [$v] - $d\n\n";
    if ($has_body) {
      $new .= $body;
      $new .= "\n" if $new !~ /\n\z/;
      if ($new !~ /Version bumped:/) {
        $new .= "\n### Changed\n$bump";
      }
    } else {
      $new .= "### Changed\n$bump";
    }

    $_ = $pre . $needle . "\n" . $new . $rest;
  ' "$f"
}

if [[ "$bump_flag" -eq 1 ]]; then
  echo "[release] target version: $ver_no_v ($today)"
  update_versions "$ver_no_v"
  update_system_prompt_header "$ver_no_v" "$today"
  update_readme_release_examples "$ver_no_v"
  check_readme_has_system_prompt_tool
  update_changelog_rollover "$ver_no_v" "$today"

  if [[ "$fmt_flag" -eq 1 ]]; then
    run "cargo fmt"
  fi
  if [[ "$test_flag" -eq 1 ]]; then
    run "cargo test -q -p buddha-mcp"
    run "cargo test -q -p buddha-core"
    run "cargo test -q -p buddha"
  fi
fi

# Commit
if [[ "$dry_run" -eq 1 ]]; then
  echo "[dry-run] git add -A && git commit -m 'chore(release): ${tag}'"
else
  git add -A
  git commit -m "chore(release): ${tag}" || echo "Nothing to commit; continuing"
fi

# Push (branch only)
if [[ "$push_flag" -eq 1 ]]; then
  run "git push origin HEAD"
fi

# Tag (optional)
if [[ "$tag_flag" -eq 1 ]]; then
  if git rev-parse "$tag" >/dev/null 2>&1; then
    echo "Tag ${tag} already exists. Skipping tag creation."
  else
    run "git tag -a \"$tag\" -m \"release ${ver_no_v}\""
  fi
  if [[ "$push_flag" -eq 1 ]]; then
    run "git push origin \"$tag\""
  fi
fi

# GitHub release (optional)
if [[ "$release_flag" -eq 1 ]]; then
  command -v gh >/dev/null 2>&1 || die "gh is required for --release"
  if [[ "$auto_notes" -eq 1 ]]; then
    run "gh release create \"$tag\" --title \"$ver_no_v\" --generate-notes --latest"
  else
    # Extract notes for this version from CHANGELOG.md and pass to gh.
    notes_file="$(mktemp -t buddha-release-notes.XXXXXX)"
    trap 'rm -f "$notes_file"' EXIT
    BUDDHA_REL_VER="$ver_no_v" perl -ne '
      our ($v,$on)=($ENV{BUDDHA_REL_VER},0);
      if (/^## \[$v\] - /) { $on=1; next }
      if ($on && /^## \[/) { exit }
      print if $on;
    ' CHANGELOG.md >"$notes_file"
    run "gh release create \"$tag\" --title \"$ver_no_v\" --notes-file \"$notes_file\" --latest"
  fi
fi

echo "Done: $tag"
