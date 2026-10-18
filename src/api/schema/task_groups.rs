use serde::{Deserialize, Serialize};

use crate::cli::projection::{cli_params, Field, Input};

fn group_id() -> Input<String> {
    Input::positional("group_id", 0, |slot, value| {
        *slot = value.to_owned();
        Ok(())
    })
    .value_name("GROUP_ID")
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct TaskGroupCreateParams => TaskGroupCreateProjection {
        pub label: String => Field::new(String::new()).input(
            Input::positional("label", 0, |slot, value| {
                *slot = value.to_owned();
                Ok(())
            }).value_name("LABEL")
        ),
        /// Optionally assign one existing workspace when creating the group.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub workspace_id: Option<String> => Field::new(None).input(
            Input::value("workspace", "WORKSPACE_ID", |slot, value| {
                *slot = Some(crate::cli::normalize_workspace_id(value));
                Ok(())
            })
        ),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct TaskGroupRenameParams => TaskGroupRenameProjection {
        pub group_id: String => Field::new(String::new()).input(group_id()),
        pub label: String => Field::new(String::new()).input(crate::cli::workspace_bindings::label()),
    }
}

cli_params! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct TaskGroupTarget => TaskGroupTargetProjection {
        pub group_id: String => Field::new(String::new()).input(group_id()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TaskGroupAssignParams {
    pub workspace_id: String,
    /// A group ID assigns or moves the workspace; null or omission removes membership.
    #[serde(default)]
    pub group_id: Option<String>,
}
