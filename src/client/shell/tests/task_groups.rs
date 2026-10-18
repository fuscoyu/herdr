use super::*;
use crate::task_groups::TaskGroup;

fn group(id: &str, label: &str, members: &[&str]) -> TaskGroup {
    TaskGroup {
        id: id.into(),
        label: label.into(),
        workspace_ids: members.iter().map(|id| (*id).to_owned()).collect(),
    }
}

fn task_snapshot() -> ClientShellSnapshot {
    let mut snapshot = snapshot();
    let template = snapshot.workspaces[0].clone();
    snapshot.workspaces = [
        ("ws_1", "bots", "main", false),
        ("bots_a", "bots", "taskA", true),
        ("common_a", "common", "taskA", true),
        ("bots_b", "bots", "taskB", true),
        ("common_b", "common", "taskB", true),
        ("common_root", "common", "main", false),
        ("bots_loose", "bots", "loose", true),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, repo, branch, linked))| {
        let mut workspace = template.clone();
        workspace.workspace_id = id.into();
        workspace.number = index + 1;
        workspace.label = format!("{repo}/{branch}");
        workspace.branch = Some(branch.into());
        workspace.focused = id == "ws_1";
        workspace.worktree = Some(ClientShellWorktree {
            key: repo.into(),
            label: repo.into(),
            is_linked_worktree: linked,
        });
        workspace
    })
    .collect();
    snapshot.task_groups = vec![
        group("a", "Task A", &["bots_a", "common_a"]),
        group("b", "Task B", &["bots_b", "common_b"]),
        group("empty", "Empty task", &[]),
    ];
    snapshot
}

fn projected_ids(snapshot: &ClientShellSnapshot, collapsed: &HashSet<String>) -> Vec<String> {
    sidebar::sidebar_entries(snapshot, collapsed)
        .into_iter()
        .map(|row| match row {
            sidebar::SidebarEntry::TaskGroup(index) => {
                format!("group:{}", snapshot.task_groups[index].id)
            }
            sidebar::SidebarEntry::Workspace(entry) => {
                snapshot.workspaces[entry.index].workspace_id.clone()
            }
        })
        .collect()
}

fn task_state() -> ClientShellState {
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.preferences_path = None;
    let mut state = ClientShellState::new(config);
    state.set_snapshot(Box::new(task_snapshot()));
    state.set_pane_surface(surface());
    state
}

fn activate_action(
    state: &mut ClientShellState,
    action: ClientContextMenuAction,
) -> ClientShellInput {
    let Some(ClientShellOverlay::ContextMenu(menu)) = state.overlay.as_ref() else {
        panic!("context menu expected");
    };
    let index = menu
        .items()
        .iter()
        .position(|item| item.action == action)
        .expect("menu action");
    let mut outcome = ClientShellInput::default();
    state.activate_context_menu_item(index, &mut outcome);
    outcome
}

#[test]
fn task_groups_project_cross_repo_members_and_preserve_unassigned_worktrees() {
    let snapshot = task_snapshot();
    assert_eq!(
        projected_ids(&snapshot, &HashSet::new()),
        [
            "group:a",
            "bots_a",
            "common_a",
            "group:b",
            "bots_b",
            "common_b",
            "group:empty",
            "ws_1",
            "bots_loose",
            "common_root",
        ]
    );
    let entries = render::workspace_entries(&snapshot, &HashSet::new());
    assert_eq!(entries.len(), snapshot.workspaces.len());
    let ids = entries
        .iter()
        .map(|entry| &snapshot.workspaces[entry.index].workspace_id)
        .collect::<HashSet<_>>();
    assert_eq!(
        ids.len(),
        snapshot.workspaces.len(),
        "each workspace appears exactly once"
    );
    assert!(
        entries
            .iter()
            .find(|entry| snapshot.workspaces[entry.index].workspace_id == "bots_loose")
            .unwrap()
            .indented
    );
    assert!(
        !entries
            .iter()
            .find(|entry| snapshot.workspaces[entry.index].workspace_id == "common_root")
            .unwrap()
            .indented
    );
}

#[test]
fn task_groups_collapse_is_namespaced_and_retains_focused_member() {
    let mut snapshot = task_snapshot();
    snapshot.workspaces[0].focused = false;
    snapshot.workspaces[1].focused = true;
    snapshot.focused_workspace_id = Some("bots_a".into());
    // Duplicate/stale memberships are ignored even when their first group is collapsed.
    snapshot.task_groups[1]
        .workspace_ids
        .extend(["common_a".into(), "missing".into()]);
    let collapsed = HashSet::from(["task-group:a".into(), "bots".into()]);
    assert_eq!(
        projected_ids(&snapshot, &collapsed),
        [
            "group:a",
            "bots_a",
            "group:b",
            "bots_b",
            "common_b",
            "group:empty",
            "ws_1",
            "common_root",
        ]
    );
    let navigation = render::workspace_entries(&snapshot, &collapsed);
    assert!(navigation
        .iter()
        .all(|entry| snapshot.workspaces.get(entry.index).is_some()));
    assert_eq!(
        navigation.iter().filter(|entry| entry.task_group).count(),
        3
    );
}

#[test]
fn task_groups_separate_worktree_close_and_status_from_other_tasks() {
    let mut snapshot = task_snapshot();
    snapshot.workspaces[1].agent_status = AgentStatus::Blocked;
    let collapsed = HashSet::from(["bots".into()]);
    assert!(!sidebar::workspace_close_is_group(
        &snapshot,
        &snapshot.workspaces[0]
    ));
    assert_eq!(
        sidebar::displayed_workspace_status(&snapshot, &snapshot.workspaces[0], &collapsed),
        AgentStatus::Idle
    );
    snapshot.task_groups[0].workspace_ids.push("ws_1".into());
    assert!(sidebar::parent_group_key(&snapshot, 0).is_none());
    assert!(!sidebar::workspace_close_is_group(
        &snapshot,
        &snapshot.workspaces[0]
    ));
    let entries = render::workspace_entries(&snapshot, &HashSet::new());
    let loose = entries
        .iter()
        .find(|entry| snapshot.workspaces[entry.index].workspace_id == "bots_loose")
        .unwrap();
    assert!(
        !loose.indented,
        "unassigned child cannot nest under an explicitly assigned parent"
    );
}

#[test]
fn task_groups_render_headers_and_member_repository_labels_without_workspace_hits() {
    let mut state = task_state();
    let frame = state.compose(120, 60).expect("task groups frame");
    let text = frame
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>();
    for label in [
        "Task A",
        "Task B",
        "Empty task",
        "bots/taskA",
        "common/taskA",
    ] {
        assert!(text.contains(label), "missing {label}");
    }
    assert_eq!(state.hits.task_groups.len(), 3);
    assert_eq!(state.hits.workspaces.len(), 7);
    for header in &state.hits.task_groups {
        assert!(!state
            .hits
            .workspaces
            .iter()
            .any(|workspace| workspace.rect == header.rect));
    }
    let header = state.hits.task_groups[0].rect;
    let result = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: header.x + 1,
        row: header.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(result.actions.is_empty());
    assert!(state.workspace_press.is_none());
    assert!(state.collapsed_groups.contains("task-group:a"));
    state.compose(120, 60).unwrap();
    assert!(!state
        .hits
        .workspaces
        .iter()
        .any(|hit| hit.workspace_id == "bots_a"));
    assert_eq!(state.hits.task_groups.len(), 3);
    let header = state.hits.task_groups[0].rect;
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: header.x + 1,
        row: header.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(
        matches!(state.overlay, Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
        target: ClientContextMenuTarget::TaskGroup { ref group_id, .. }, ..
    })) if group_id == "a")
    );
}

#[test]
fn task_groups_context_actions_create_move_remove_rename_and_delete_only_metadata() {
    use crate::api::schema::Method;
    let mut state = task_state();
    state.open_workspace_context_menu("bots_a".into(), 0, 0);
    activate_action(&mut state, ClientContextMenuAction::NewTaskGroup);
    let Some(ClientShellOverlay::Rename(rename)) = state.overlay.as_mut() else {
        panic!("create input");
    };
    rename.input = TextEditor::new(" New task ", false);
    let mut created = ClientShellInput::default();
    state.save_rename_overlay(&mut created);
    assert!(
        matches!(&created.actions[..], [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, Method::TaskGroupCreate(params)
            if params.label == "New task" && params.workspace_id.as_deref() == Some("bots_a")))
    );

    state.open_workspace_context_menu("bots_a".into(), 0, 0);
    activate_action(&mut state, ClientContextMenuAction::ChooseTaskGroup);
    let moved = activate_action(&mut state, ClientContextMenuAction::AssignTaskGroup(0));
    assert!(
        matches!(&moved.actions[..], [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, Method::TaskGroupAssign(params)
            if params.workspace_id == "bots_a" && params.group_id.as_deref() == Some("b")))
    );

    state.open_workspace_context_menu("bots_a".into(), 0, 0);
    let removed = activate_action(&mut state, ClientContextMenuAction::RemoveFromTaskGroup);
    assert!(
        matches!(&removed.actions[..], [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, Method::TaskGroupAssign(params)
            if params.workspace_id == "bots_a" && params.group_id.is_none()))
    );

    state.open_task_group_context_menu("a".into(), 0, 0);
    activate_action(&mut state, ClientContextMenuAction::Rename);
    let Some(ClientShellOverlay::Rename(rename)) = state.overlay.as_mut() else {
        panic!("rename input");
    };
    rename.input = TextEditor::new("Renamed", false);
    let mut renamed = ClientShellInput::default();
    state.save_rename_overlay(&mut renamed);
    assert!(
        matches!(&renamed.actions[..], [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, Method::TaskGroupRename(params)
            if params.group_id == "a" && params.label == "Renamed"))
    );

    state.open_task_group_context_menu("a".into(), 0, 0);
    let deleted = activate_action(&mut state, ClientContextMenuAction::DeleteTaskGroup);
    assert!(
        matches!(&deleted.actions[..], [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, Method::TaskGroupDelete(target) if target.group_id == "a"))
    );
    assert_eq!(state.snapshot.as_ref().unwrap().workspaces.len(), 7);
}

#[test]
fn task_groups_blank_area_menu_can_create_empty_group_and_escape_discards_input() {
    let mut state = task_state();
    state.open_task_groups_context_menu(2, 15);
    activate_action(&mut state, ClientContextMenuAction::NewTaskGroup);
    assert!(matches!(
        state.overlay,
        Some(ClientShellOverlay::Rename(ClientRenameOverlay {
            target: ClientRenameTarget::NewTaskGroup { workspace_id: None },
            ..
        }))
    ));
    let outcome = state.handle_input_bytes(b"\x1b");
    assert!(outcome.actions.is_empty());
    assert!(state.overlay.is_none());
}

#[test]
fn task_groups_missing_methods_hide_only_unsupported_group_actions() {
    let mut state = task_state();
    state.set_endpoint_methods(Some(vec![
        "workspace.rename".into(),
        "workspace.close".into(),
        "task_group.rename".into(),
    ]));
    state.open_workspace_context_menu("bots_a".into(), 0, 0);
    let Some(ClientShellOverlay::ContextMenu(menu)) = state.overlay.as_ref() else {
        panic!("workspace menu");
    };
    assert!(menu
        .items()
        .iter()
        .any(|item| item.action == ClientContextMenuAction::Rename));
    assert!(!menu.items().iter().any(|item| matches!(
        item.action,
        ClientContextMenuAction::NewTaskGroup
            | ClientContextMenuAction::ChooseTaskGroup
            | ClientContextMenuAction::RemoveFromTaskGroup
    )));
    state.open_task_group_context_menu("a".into(), 0, 0);
    let Some(ClientShellOverlay::ContextMenu(menu)) = state.overlay.as_ref() else {
        panic!("group menu");
    };
    assert!(menu
        .items()
        .iter()
        .any(|item| item.action == ClientContextMenuAction::Rename));
    assert!(menu
        .items()
        .iter()
        .any(|item| item.action == ClientContextMenuAction::ToggleGroup));
    assert!(!menu
        .items()
        .iter()
        .any(|item| item.action == ClientContextMenuAction::DeleteTaskGroup));
}

#[test]
fn task_groups_long_picker_keeps_keyboard_selected_row_visible() {
    let menu = ClientContextMenuOverlay {
        target: ClientContextMenuTarget::TaskGroupPicker {
            workspace_id: "ws_1".into(),
            groups: (0..30)
                .map(|index| group(&index.to_string(), &format!("Task {index}"), &[]))
                .collect(),
        },
        x: 1,
        y: 1,
        highlighted: 29,
    };
    let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 8));
    let rendered = render::render_context_menu(&mut buffer, &menu, &Palette::terminal()).unwrap();
    assert!(rendered.menu_rows.iter().any(|(_, index)| *index == 29));
    assert!(rendered.menu_rows.len() <= 6);
}

#[test]
fn task_groups_endpoint_headers_collapse_independently_and_hide_with_machine() {
    use crate::client::endpoint::ProfileId;
    let mut state = task_state();
    let profile = SavedSshEndpoint {
        id: ProfileId::parse("0123456789abcdef0123456789abcdef").unwrap(),
        label: "Build".into(),
        target: "dev@build.example".into(),
        session: "agents".into(),
        enabled: true,
    };
    let remote_id = ClientEndpointId::Ssh(profile.id.clone());
    state.set_endpoint_catalog(&[profile]);
    state.set_endpoint_status(&remote_id, ClientEndpointStatus::Online);
    let mut remote = task_snapshot();
    remote.boot_id = "remote-boot".into();
    state.set_endpoint_snapshot(&remote_id, Box::new(remote));
    state.toggle_collapsed_group(&remote_id, "task-group:a".into());
    state.compose(120, 90).unwrap();
    assert!(state
        .hits
        .workspaces
        .iter()
        .any(|hit| hit.endpoint_id.is_local() && hit.workspace_id == "bots_a"));
    assert!(!state
        .hits
        .workspaces
        .iter()
        .any(|hit| hit.endpoint_id == remote_id && hit.workspace_id == "bots_a"));
    assert_eq!(
        state
            .hits
            .task_groups
            .iter()
            .filter(|hit| hit.endpoint_id == remote_id)
            .count(),
        3
    );
    assert!(!state.collapsed_groups.contains("task-group:a"));
    state.collapsed_endpoints.insert(remote_id.clone());
    state.compose(120, 90).unwrap();
    assert!(!state
        .hits
        .task_groups
        .iter()
        .any(|hit| hit.endpoint_id == remote_id));
    assert_eq!(state.hits.task_groups.len(), 3);
}

#[test]
fn task_groups_preserve_unassigned_drag_but_never_drag_assigned_workspaces() {
    let mut state = task_state();
    assert!(
        state.endpoint_workspace_is_draggable(&ClientWorkspacePress {
            endpoint_id: ClientEndpointId::Local,
            workspace_id: "ws_1".into(),
            start_column: 1,
            start_row: 1,
        })
    );
    assert!(
        !state.endpoint_workspace_is_draggable(&ClientWorkspacePress {
            endpoint_id: ClientEndpointId::Local,
            workspace_id: "bots_a".into(),
            start_column: 1,
            start_row: 1,
        })
    );
    state.compose(120, 60).unwrap();
    let parent = state
        .hits
        .workspaces
        .iter()
        .find(|hit| hit.workspace_id == "ws_1")
        .unwrap()
        .rect;
    let other = state
        .hits
        .workspaces
        .iter()
        .find(|hit| hit.workspace_id == "common_root")
        .unwrap()
        .rect;
    for (kind, rect, row) in [
        (MouseEventKind::Down(MouseButton::Left), parent, parent.y),
        (
            MouseEventKind::Drag(MouseButton::Left),
            other,
            other.bottom(),
        ),
    ] {
        state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
            kind,
            column: rect.x + 2,
            row,
            modifiers: KeyModifiers::empty(),
        })]);
    }
    let moved = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: other.x + 2,
        row: other.bottom(),
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(
        matches!(&moved.actions[..], [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, crate::api::schema::Method::WorkspaceMoveBlock(params)
            if params.workspace_ids == ["ws_1", "bots_loose"] && params.before_workspace_id.is_none()))
    );
}

#[test]
fn task_groups_empty_groups_preserve_existing_workspace_drag() {
    let mut snapshot = snapshot();
    snapshot.task_groups = vec![group("empty", "Empty task", &[])];
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot));
    assert!(
        state.endpoint_workspace_is_draggable(&ClientWorkspacePress {
            endpoint_id: ClientEndpointId::Local,
            workspace_id: "ws_1".into(),
            start_column: 1,
            start_row: 1,
        })
    );
}

#[test]
fn task_groups_mouse_capture_off_removes_group_hits() {
    let mut state = task_state();
    state.config.mouse_capture = false;
    state.compose(120, 60).unwrap();
    assert!(state.hits.task_groups.is_empty());
    assert!(state.hits.workspaces.is_empty());
}

#[test]
fn task_groups_collapsed_preferences_round_trip_locally_and_per_endpoint() {
    let path = std::env::temp_dir().join(format!(
        "herdr-task-group-prefs-{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let profile = SavedSshEndpoint::new("Build", "dev@build.example", "agents").unwrap();
    let remote_id = ClientEndpointId::Ssh(profile.id.clone());
    let mut state = ClientShellState::new(
        ClientShellConfig::from_config(&Config::default()).with_preferences_path(path.clone()),
    );
    state.toggle_collapsed_group(&ClientEndpointId::Local, "task-group:a".into());
    state.toggle_collapsed_group(&ClientEndpointId::Local, "a".into());
    state.toggle_collapsed_group(&remote_id, "task-group:b".into());
    state.persist_chrome_preferences(&mut ClientShellInput::default());
    let restored = ClientShellState::new(
        ClientShellConfig::from_config(&Config::default()).with_preferences_path(path.clone()),
    );
    assert!(restored.group_is_collapsed(&ClientEndpointId::Local, "task-group:a"));
    assert!(restored.group_is_collapsed(&ClientEndpointId::Local, "a"));
    assert!(!restored.group_is_collapsed(&ClientEndpointId::Local, "task-group:b"));
    assert!(restored.group_is_collapsed(&remote_id, "task-group:b"));
    assert!(!restored.group_is_collapsed(&remote_id, "task-group:a"));
    std::fs::remove_file(path).expect("remove preference test file");
}

fn scaling_snapshot(count: usize, grouped: bool) -> ClientShellSnapshot {
    let mut snapshot = snapshot();
    let workspace = snapshot.workspaces[0].clone();
    let tab = snapshot.tabs[0].clone();
    let pane = snapshot.panes[0].clone();
    snapshot.workspaces.clear();
    snapshot.tabs.clear();
    snapshot.panes.clear();
    for index in 1..=count {
        let mut workspace = workspace.clone();
        workspace.workspace_id = format!("ws_{index}");
        workspace.active_tab_id = format!("tab_{index}");
        workspace.number = index;
        workspace.label = format!("repo-{index}");
        workspace.focused = index == 1;
        let mut tab = tab.clone();
        tab.tab_id = workspace.active_tab_id.clone();
        tab.workspace_id = workspace.workspace_id.clone();
        tab.focused = workspace.focused;
        let mut pane = pane.clone();
        pane.pane_id = format!("pane_{index}");
        pane.workspace_id = workspace.workspace_id.clone();
        pane.tab_id = tab.tab_id.clone();
        pane.focused = workspace.focused;
        if grouped {
            snapshot.task_groups.push(group(
                &format!("g{index}"),
                &format!("Task {index}"),
                &[&workspace.workspace_id],
            ));
        }
        snapshot.workspaces.push(workspace);
        snapshot.tabs.push(tab);
        snapshot.panes.push(pane);
    }
    snapshot
}

#[test]
fn task_groups_long_navigation_reveals_workspaces_after_header_rows() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(scaling_snapshot(25, true)));
    state.set_pane_surface(surface());
    state.compose(120, 24).unwrap();
    state.handle_input_bytes(&[0x02]);
    state.handle_input_bytes(b"w");
    assert_eq!(state.mode, ClientShellMode::Navigate);
    for index in 2..=25 {
        let outcome = state.handle_input_bytes(b"\x1b[B");
        assert!(outcome.actions.is_empty());
        state.compose(120, 24).unwrap();
        let id = format!("ws_{index}");
        assert!(state
            .navigate_workspace_id
            .as_ref()
            .is_some_and(|target| target.workspace_id == id));
        assert!(
            state
                .hits
                .workspaces
                .iter()
                .any(|hit| hit.workspace_id == id),
            "navigation row {index} hidden behind header offsets"
        );
    }
}

#[test]
#[ignore = "supporting full-shell render scaling profile; run explicitly with --ignored --nocapture"]
fn task_groups_render_scale_profile() {
    use std::{hint::black_box, time::Instant};
    const ITERATIONS: u32 = 1_000;
    for count in [1, 15] {
        let mut timings = Vec::new();
        for grouped in [false, true] {
            let mut state =
                ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
            state.set_snapshot(Box::new(scaling_snapshot(count, grouped)));
            state.set_pane_surface(surface());
            for _ in 0..20 {
                black_box(state.compose(120, 50).unwrap());
            }
            let start = Instant::now();
            for _ in 0..ITERATIONS {
                black_box(state.compose(120, 50).unwrap());
            }
            let elapsed = start.elapsed() / ITERATIONS;
            eprintln!(
                "task-group full-shell render: {count} workspaces/panes, grouped={grouped}, {elapsed:?}/frame"
            );
            timings.push(elapsed);
        }
        eprintln!(
            "task-group full-shell render: {count} workspaces/panes, grouped/baseline={:.3}",
            timings[1].as_secs_f64() / timings[0].as_secs_f64()
        );
    }
}
