#!/usr/bin/env bash
set -euo pipefail

readonly ARG_JOB="${1:-}"
readonly ARG_STEP="${2:-}"
readonly GIT_REPOSITORY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

function help() {
    echo "USAGE: ${0} <job__check> [step__check | step__test | step__build]"
    echo "       ${0} prepare"
    echo "       ${0} run [workspace-directory ...]"
    echo ""
    echo "Builds prepare material-icon-theme@5.40.0 with Bun; prepared resources are reused offline."
    echo "Before invoking cargo directly, run: bash script/grove.sh prepare"
}

function prepare() {
    if ! command -v bun > /dev/null; then
        echo "error: install Bun to prepare material-icon-theme@5.40.0" >&2
        return 1
    fi
    bun "${GIT_REPOSITORY_DIR}/script/material/prepare.ts"
}

function step__check() {
    prepare
    cargo fmt --manifest-path "${GIT_REPOSITORY_DIR}/Cargo.toml" --all --check
    cargo clippy --manifest-path "${GIT_REPOSITORY_DIR}/Cargo.toml" --workspace --all-targets -- -D warnings
}

function step__test() {
    prepare
    cargo test --manifest-path "${GIT_REPOSITORY_DIR}/Cargo.toml" --workspace
}

function step__build() {
    prepare
    cargo build --manifest-path "${GIT_REPOSITORY_DIR}/Cargo.toml" --workspace
}

function job__check() {
    case "${ARG_STEP}" in
    step__check) step__check ;;
    step__test) step__test ;;
    step__build) step__build ;;
    *)
        step__check
        step__test
        step__build
        ;;
    esac
}

function run() {
    shift
    prepare
    cargo run --manifest-path "${GIT_REPOSITORY_DIR}/Cargo.toml" --package grove -- "${@}"
}

case "${ARG_JOB}" in
job__check) job__check ;;
prepare) prepare ;;
run) run "${@}" ;;
*) help ;;
esac
