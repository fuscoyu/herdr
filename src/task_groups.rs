//! Explicit, session-owned task organization. Membership refers to a workspace,
//! never to a repository or Git common directory. This is not filesystem isolation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskGroup {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub workspace_ids: Vec<String>,
}

pub(crate) fn normalized_label(label: String) -> Result<String, String> {
    let label = label.trim();
    if label.is_empty() {
        return Err("task group label must not be empty".into());
    }
    if label.chars().any(char::is_control) || label.chars().count() > 128 {
        return Err(
            "task group label must be at most 128 characters without control characters".into(),
        );
    }
    Ok(label.to_owned())
}

pub(crate) fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("g{time:x}{:x}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
}
