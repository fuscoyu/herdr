use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskGroupCreateParams {
    pub label: String,
    /// Optionally assign one existing workspace when creating the group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskGroupRenameParams {
    pub group_id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskGroupTarget {
    pub group_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskGroupAssignParams {
    pub workspace_id: String,
    /// A group ID assigns or moves the workspace; null or omission removes membership.
    #[serde(default)]
    pub group_id: Option<String>,
}
