use clap::{Arg, Command};

use crate::api::schema::{
    EmptyParams, GroupAssignParams, GroupCreateParams, GroupRenameParams, GroupTarget, Method,
    Request,
};

pub(crate) fn command() -> Command {
    let group_id = || Arg::new("group_id").value_name("GROUP_ID").required(true);
    let workspace_id = || {
        Arg::new("workspace_id")
            .value_name("WORKSPACE_ID")
            .required(true)
    };
    Command::new("group")
        .about("Organize workspaces into named groups")
        .subcommand_required(true)
        .subcommand(Command::new("list").about("List groups and their workspace IDs"))
        .subcommand(
            Command::new("create")
                .about("Create a named group, optionally with a workspace")
                .arg(Arg::new("label").value_name("GROUP_NAME").required(true))
                .arg(
                    Arg::new("workspace")
                        .long("workspace")
                        .value_name("WORKSPACE_ID"),
                ),
        )
        .subcommand(
            Command::new("rename")
                .about("Rename a group")
                .arg(group_id())
                .arg(
                    Arg::new("label")
                        .value_name("GROUP_NAME")
                        .required(true)
                        .num_args(1..)
                        .trailing_var_arg(true)
                        .allow_hyphen_values(true),
                ),
        )
        .subcommand(
            Command::new("delete")
                .about("Delete a group without closing its workspaces")
                .arg(group_id()),
        )
        .subcommand(
            Command::new("add")
                .about("Assign or move a workspace to a group")
                .arg(group_id())
                .arg(workspace_id()),
        )
        .subcommand(
            Command::new("remove")
                .about("Remove a workspace from its group")
                .arg(workspace_id()),
        )
}

fn request(args: &[String]) -> Result<Request, clap::Error> {
    let matches = command()
        .try_get_matches_from(std::iter::once("group").chain(args.iter().map(String::as_str)))?;
    let (name, matches) = matches.subcommand().expect("subcommand required by clap");
    let value = |key: &str| {
        matches
            .get_one::<String>(key)
            .expect("argument required by clap")
            .clone()
    };
    let method = match name {
        "list" => Method::GroupList(EmptyParams {}),
        "create" => Method::GroupCreate(GroupCreateParams {
            label: value("label"),
            workspace_id: matches
                .get_one::<String>("workspace")
                .map(|id| super::normalize_workspace_id(id)),
        }),
        "rename" => Method::GroupRename(GroupRenameParams {
            group_id: value("group_id"),
            label: matches
                .get_many::<String>("label")
                .expect("label required by clap")
                .cloned()
                .collect::<Vec<_>>()
                .join(" "),
        }),
        "delete" => Method::GroupDelete(GroupTarget {
            group_id: value("group_id"),
        }),
        "add" => Method::GroupAssign(GroupAssignParams {
            group_id: Some(value("group_id")),
            workspace_id: super::normalize_workspace_id(&value("workspace_id")),
        }),
        "remove" => Method::GroupAssign(GroupAssignParams {
            group_id: None,
            workspace_id: super::normalize_workspace_id(&value("workspace_id")),
        }),
        _ => unreachable!("subcommand validated by clap"),
    };
    Ok(Request {
        id: format!("cli:group:{name}"),
        method,
    })
}

pub(super) fn run_group_command(args: &[String]) -> std::io::Result<i32> {
    match request(args) {
        Ok(request) => super::print_response(&super::send_request(&request)?),
        Err(error) => {
            let exit_code = error.exit_code();
            error.print()?;
            Ok(exit_code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Request, clap::Error> {
        request(&args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn group_cli_projects_all_commands_to_json_api() {
        for (args, method, params) in [
            (vec!["list"], "group.list", serde_json::json!({})),
            (
                vec!["create", "Cross-repo change"],
                "group.create",
                serde_json::json!({"label":"Cross-repo change"}),
            ),
            (
                vec!["create", "Release", "--workspace=w2"],
                "group.create",
                serde_json::json!({"label":"Release","workspace_id":"w2"}),
            ),
            (
                vec!["rename", "g1", "Release", "review"],
                "group.rename",
                serde_json::json!({"group_id":"g1","label":"Release review"}),
            ),
            (
                vec!["delete", "g1"],
                "group.delete",
                serde_json::json!({"group_id":"g1"}),
            ),
            (
                vec!["add", "g1", "2"],
                "group.assign",
                serde_json::json!({"group_id":"g1","workspace_id":"2"}),
            ),
            (
                vec!["remove", "w2"],
                "group.assign",
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
    fn group_cli_rejects_invalid_grammar_before_transport() {
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
