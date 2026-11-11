#![cfg(target_os = "macos")]
// This narrow native acceptance target reuses shared CLI utilities; most are
// intentionally unused because the broad CLI entry point excludes macOS.
#![allow(dead_code, unused_imports)]

pub mod support;

#[path = "cli/harness.rs"]
mod harness;
#[path = "cli/task_groups.rs"]
mod task_groups;
