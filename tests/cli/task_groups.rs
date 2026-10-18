use super::harness::*;

fn group_cli(socket_path: &Path, args: &[&str]) -> serde_json::Value {
    let output = run_cli(socket_path, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn task_group_cli_cross_repo_worktrees_survive_restart_and_group_deletion() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = named_session_socket(&config_home, "task-groups");
    let repos = [base.join("bots"), base.join("common")];
    for repo in &repos {
        create_committed_repo(repo);
    }
    let mut server = spawn_named_server(&config_home, &runtime_dir, "task-groups");
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let mut checkouts = Vec::new();
    let mut members = [Vec::new(), Vec::new()];
    for (repo_index, repo) in repos.iter().enumerate() {
        for (task_index, branch) in ["task-a", "task-b"].iter().enumerate() {
            let checkout = base.join(format!("checkout-{repo_index}-{branch}"));
            let created = group_cli(
                &socket_path,
                &[
                    "worktree",
                    "create",
                    "--cwd",
                    repo.to_str().unwrap(),
                    "--branch",
                    branch,
                    "--path",
                    checkout.to_str().unwrap(),
                    "--json",
                ],
            );
            let workspace_id = created["result"]["workspace"]["workspace_id"]
                .as_str()
                .unwrap()
                .to_owned();
            assert_eq!(
                created["result"]["workspace"]["worktree"]["is_linked_worktree"],
                true
            );
            members[task_index].push(workspace_id);
            fs::write(
                checkout.join("uncommitted-task.txt"),
                format!("{repo_index}/{branch}"),
            )
            .unwrap();
            checkouts.push((checkout, repo_index, *branch));
        }
    }
    let workspace_list = group_cli(&socket_path, &["workspace", "list"]);
    let workspace_count = workspace_list["result"]["workspaces"]
        .as_array()
        .unwrap()
        .len();
    let first = group_cli(
        &socket_path,
        &["group", "create", "Task A", "--workspace", &members[0][0]],
    );
    let first_id = first["result"]["group"]["id"].as_str().unwrap().to_owned();
    group_cli(&socket_path, &["group", "add", &first_id, &members[0][1]]);
    let second = group_cli(
        &socket_path,
        &["group", "create", "Task B", "--workspace", &members[1][0]],
    );
    let second_id = second["result"]["group"]["id"].as_str().unwrap().to_owned();
    group_cli(&socket_path, &["group", "add", &second_id, &members[1][1]]);
    group_cli(&socket_path, &["group", "create", "Empty task"]);
    let renamed = group_cli(
        &socket_path,
        &["group", "rename", &second_id, "Task", "B review"],
    );
    assert_eq!(renamed["result"]["group"]["label"], "Task B review");

    // Moving a linked checkout never moves its same-repository siblings.
    group_cli(&socket_path, &["group", "add", &second_id, &members[0][0]]);
    let listed = group_cli(&socket_path, &["group", "list"]);
    assert_eq!(
        listed["result"]["groups"][0]["workspace_ids"],
        serde_json::json!([members[0][1]])
    );
    assert_eq!(
        listed["result"]["groups"][1]["workspace_ids"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    group_cli(&socket_path, &["group", "remove", &members[0][0]]);
    group_cli(&socket_path, &["group", "add", &first_id, &members[0][0]]);
    let before_restart = group_cli(&socket_path, &["group", "list"]);
    assert_eq!(
        before_restart["result"]["groups"][1]["workspace_ids"],
        serde_json::json!(members[1])
    );

    let invalid = send_request(
        &socket_path,
        &serde_json::json!({
            "id":"invalid", "method":"task_group.assign", "params":{
                "workspace_id":members[0][0], "group_id":"missing-group",
            },
        })
        .to_string(),
    );
    assert_eq!(invalid["error"]["code"], "task_group_not_found");
    let invalid = run_cli(&socket_path, &["group", "create", " "]);
    assert_eq!(invalid.status.code(), Some(1));

    let stopped = run_cli(&socket_path, &["server", "stop"]);
    assert!(
        stopped.status.success(),
        "{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    assert!(server.wait_for_exit().success());
    drop(server);
    let mut restarted = spawn_named_server(&config_home, &runtime_dir, "task-groups");
    wait_for_socket(&socket_path, Duration::from_secs(5));
    let restored = group_cli(&socket_path, &["group", "list"]);
    assert_eq!(restored["result"], before_restart["result"]);

    group_cli(&socket_path, &["group", "delete", &first_id]);
    let listed = group_cli(&socket_path, &["group", "list"]);
    assert_eq!(listed["result"]["groups"].as_array().unwrap().len(), 2);
    assert_eq!(
        listed["result"]["groups"][0],
        before_restart["result"]["groups"][1]
    );
    assert_eq!(
        listed["result"]["groups"][1]["workspace_ids"],
        serde_json::json!([])
    );
    let workspaces = group_cli(&socket_path, &["workspace", "list"]);
    assert_eq!(
        workspaces["result"]["workspaces"].as_array().unwrap().len(),
        workspace_count
    );
    for workspace_id in members.iter().flatten() {
        let fetched = group_cli(&socket_path, &["workspace", "get", workspace_id]);
        assert_eq!(
            fetched["result"]["workspace"]["workspace_id"],
            *workspace_id
        );
    }
    for (checkout, repo_index, branch) in checkouts {
        assert!(checkout.join("README.md").exists());
        assert_eq!(
            fs::read_to_string(checkout.join("uncommitted-task.txt")).unwrap(),
            format!("{repo_index}/{branch}")
        );
        run_git(
            &repos[repo_index],
            &["show-ref", "--verify", &format!("refs/heads/{branch}")],
        );
        run_git(&checkout, &["rev-parse", "--is-inside-work-tree"]);
    }
    assert!(run_cli(&socket_path, &["server", "stop"]).status.success());
    assert!(restarted.wait_for_exit().success());
    drop(restarted);
    cleanup_test_base(&base);
}

#[test]
fn task_group_cli_help_and_invalid_arguments_work_without_a_server() {
    let base = unique_test_dir();
    let socket_path = base.join("missing.sock");
    for args in [
        vec!["group", "create"],
        vec!["group", "rename", "g1"],
        vec!["group", "add", "g1"],
        vec!["group", "remove"],
        vec!["group", "delete", "g1", "extra"],
    ] {
        let output = run_cli(&socket_path, &args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("server is not running"));
    }
    let help = run_cli(&socket_path, &["group", "--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Organize workspaces"));
    cleanup_test_base(&base);
}
