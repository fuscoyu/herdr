use clap::Command;

use crate::api::schema::task_groups::{
    TaskGroupCreateProjection, TaskGroupRenameProjection, TaskGroupTargetProjection,
};
use crate::api::schema::{EmptyParams, Method, Request, TaskGroupAssignParams};

use super::projection::{self, cli_projection, Field, Input, Leaf, ParseError};

cli_projection! {
    TaskGroupListProjection for EmptyParams {}
}

cli_projection! {
    TaskGroupAddProjection for TaskGroupAssignParams {
        group_id => Field::new(None).input(Input::positional("group_id", 0, |slot, value| {
            *slot = Some(value.to_owned());
            Ok(())
        }).value_name("GROUP_ID")),
        workspace_id => Field::new(String::new()).input(Input::positional("workspace_id", 1, |slot, value| {
            *slot = super::normalize_workspace_id(value);
            Ok(())
        }).value_name("WORKSPACE_ID")),
    }
}

cli_projection! {
    TaskGroupRemoveProjection for TaskGroupAssignParams {
        group_id => Field::new(None::<String>),
        workspace_id => Field::new(String::new()).input(super::workspace_bindings::workspace_id().value_name("WORKSPACE_ID")),
    }
}

const LEAVES: [Leaf; 6] = [
    Leaf {
        name: "list",
        about: "List task groups and their workspace IDs",
        id: "cli:group:list",
        arguments: TaskGroupListProjection::arguments,
        method: |matches| TaskGroupListProjection::from_matches(matches).map(Method::TaskGroupList),
    },
    Leaf {
        name: "create",
        about: "Create a named task group, optionally with a workspace",
        id: "cli:group:create",
        arguments: TaskGroupCreateProjection::arguments,
        method: |matches| {
            TaskGroupCreateProjection::from_matches(matches).map(Method::TaskGroupCreate)
        },
    },
    Leaf {
        name: "rename",
        about: "Rename a task group",
        id: "cli:group:rename",
        arguments: TaskGroupRenameProjection::arguments,
        method: |matches| {
            TaskGroupRenameProjection::from_matches(matches).map(Method::TaskGroupRename)
        },
    },
    Leaf {
        name: "delete",
        about: "Delete a task group without closing its workspaces",
        id: "cli:group:delete",
        arguments: TaskGroupTargetProjection::arguments,
        method: |matches| {
            TaskGroupTargetProjection::from_matches(matches).map(Method::TaskGroupDelete)
        },
    },
    Leaf {
        name: "add",
        about: "Assign or move a workspace to a task group",
        id: "cli:group:add",
        arguments: TaskGroupAddProjection::arguments,
        method: |matches| {
            TaskGroupAddProjection::from_matches(matches).map(Method::TaskGroupAssign)
        },
    },
    Leaf {
        name: "remove",
        about: "Remove a workspace from its task group",
        id: "cli:group:remove",
        arguments: TaskGroupRemoveProjection::arguments,
        method: |matches| {
            TaskGroupRemoveProjection::from_matches(matches).map(Method::TaskGroupAssign)
        },
    },
];

pub(crate) fn command() -> Command {
    Command::new("group")
        .about("Organize workspaces into named task groups")
        .subcommand_required(true)
        .subcommands(LEAVES.iter().map(Leaf::command))
}

fn request(args: &[String]) -> Result<Request, ParseError> {
    projection::group_request(command(), &LEAVES, args)
}

pub(super) fn run_group_command(args: &[String]) -> std::io::Result<i32> {
    super::run_json_request(request(args))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Request, ParseError> {
        request(&args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn task_group_cli_projects_all_commands_to_json_api() {
        for (args, method, params) in [
            (vec!["list"], "task_group.list", serde_json::json!({})),
            (
                vec!["create", "Cross-repo change"],
                "task_group.create",
                serde_json::json!({"label":"Cross-repo change"}),
            ),
            (
                vec!["create", "Release", "--workspace=w2"],
                "task_group.create",
                serde_json::json!({"label":"Release","workspace_id":"w2"}),
            ),
            (
                vec!["rename", "g1", "Release", "review"],
                "task_group.rename",
                serde_json::json!({"group_id":"g1","label":"Release review"}),
            ),
            (
                vec!["delete", "g1"],
                "task_group.delete",
                serde_json::json!({"group_id":"g1"}),
            ),
            (
                vec!["add", "g1", "2"],
                "task_group.assign",
                serde_json::json!({"group_id":"g1","workspace_id":"2"}),
            ),
            (
                vec!["remove", "w2"],
                "task_group.assign",
                serde_json::json!({"group_id":null,"workspace_id":"w2"}),
            ),
        ] {
            let json = serde_json::to_value(parse(&args).unwrap()).unwrap();
            assert_eq!(json["method"], method, "{args:?}");
            assert_eq!(json["params"], params, "{args:?}");
        }
        command().debug_assert();
    }

    #[test]
    fn task_group_cli_rejects_invalid_grammar_before_transport() {
        for args in [
            vec![],
            vec!["unknown"],
            vec!["create"],
            vec!["create", "Release", "--workspace"],
            vec!["rename", "g1"],
            vec!["delete"],
            vec!["delete", "g1", "extra"],
            vec!["add", "g1"],
            vec!["remove"],
            vec!["remove", "w1", "extra"],
            vec!["list", "extra"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
}
