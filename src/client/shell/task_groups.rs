use super::*;

pub(super) fn collapse_key(group_id: &str) -> String {
    format!("task-group:{group_id}")
}

pub(super) fn assigned_workspace_ids(snapshot: &ClientShellSnapshot) -> HashSet<&str> {
    snapshot
        .task_groups
        .iter()
        .flat_map(|group| group.workspace_ids.iter().map(String::as_str))
        .collect()
}

pub(super) fn workspace_is_assigned(snapshot: &ClientShellSnapshot, workspace_id: &str) -> bool {
    snapshot
        .task_groups
        .iter()
        .any(|group| group.workspace_ids.iter().any(|id| id == workspace_id))
}

pub(super) fn render_group_header(
    buffer: &mut Buffer,
    rect: Rect,
    group: &crate::task_groups::TaskGroup,
    collapsed: bool,
    endpoint_id: &ClientEndpointId,
    palette: &Palette,
    hits: &mut ShellHitMap,
) {
    render::put_text(
        buffer,
        rect.x,
        rect.y,
        rect.width,
        &format!(" {} {}", if collapsed { "▸" } else { "▾" }, group.label),
        Style::default()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD),
    );
    hits.task_groups.push(TaskGroupHit {
        rect,
        endpoint_id: endpoint_id.clone(),
        group_id: group.id.clone(),
    });
}

impl ClientShellState {
    pub(super) fn open_task_group_context_menu(&mut self, group_id: String, x: u16, y: u16) {
        use crate::api::schema::{Method, TaskGroupRenameParams, TaskGroupTarget};
        if !self.snapshot.as_deref().is_some_and(|snapshot| {
            snapshot
                .task_groups
                .iter()
                .any(|group| group.id == group_id)
        }) {
            return;
        }
        let collapsed = self.group_is_collapsed(&self.active_endpoint_id, &collapse_key(&group_id));
        let can_rename =
            self.supports_endpoint_method(&Method::TaskGroupRename(TaskGroupRenameParams {
                group_id: group_id.clone(),
                label: String::new(),
            }));
        let can_delete = self.supports_endpoint_method(&Method::TaskGroupDelete(TaskGroupTarget {
            group_id: group_id.clone(),
        }));
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::TaskGroup {
                group_id,
                collapsed,
                can_rename,
                can_delete,
            },
            x,
            y,
            highlighted: 0,
        }));
    }

    pub(super) fn open_task_groups_context_menu(&mut self, x: u16, y: u16) {
        if !self.supports_endpoint_method(&crate::api::schema::Method::TaskGroupCreate(
            crate::api::schema::TaskGroupCreateParams {
                label: String::new(),
                workspace_id: None,
            },
        )) {
            return;
        }
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::TaskGroups,
            x,
            y,
            highlighted: 0,
        }));
    }

    pub(super) fn begin_new_task_group(&mut self, workspace_id: Option<String>) {
        self.overlay = Some(ClientShellOverlay::Rename(ClientRenameOverlay {
            title: "new task group",
            input: TextEditor::new("", false),
            target: ClientRenameTarget::NewTaskGroup { workspace_id },
        }));
    }

    pub(super) fn choose_task_group(&mut self, workspace_id: String) {
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        let groups = snapshot
            .task_groups
            .iter()
            .filter(|group| !group.workspace_ids.contains(&workspace_id))
            .cloned()
            .collect::<Vec<_>>();
        if groups.is_empty() {
            return;
        }
        let rect = self
            .hits
            .workspaces
            .iter()
            .find(|hit| {
                hit.endpoint_id == self.active_endpoint_id && hit.workspace_id == workspace_id
            })
            .map(|hit| hit.rect)
            .unwrap_or_default();
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::TaskGroupPicker {
                workspace_id,
                groups,
            },
            x: rect.x,
            y: rect.y,
            highlighted: 0,
        }));
    }

    pub(super) fn assign_task_group(
        &mut self,
        workspace_id: String,
        group_id: Option<String>,
        outcome: &mut ClientShellInput,
    ) {
        self.push_endpoint_method(
            crate::api::schema::Method::TaskGroupAssign(
                crate::api::schema::TaskGroupAssignParams {
                    workspace_id,
                    group_id,
                },
            ),
            outcome,
        );
    }

    pub(super) fn activate_task_group_context_action(
        &mut self,
        group_id: String,
        action: ClientContextMenuAction,
        outcome: &mut ClientShellInput,
    ) {
        match action {
            ClientContextMenuAction::Rename => {
                if let Some(group) = self.snapshot.as_deref().and_then(|snapshot| {
                    snapshot
                        .task_groups
                        .iter()
                        .find(|group| group.id == group_id)
                }) {
                    self.overlay = Some(ClientShellOverlay::Rename(ClientRenameOverlay {
                        title: "rename task group",
                        input: TextEditor::new(&group.label, false),
                        target: ClientRenameTarget::TaskGroup { group_id },
                    }));
                }
            }
            ClientContextMenuAction::DeleteTaskGroup => self.push_endpoint_method(
                crate::api::schema::Method::TaskGroupDelete(crate::api::schema::TaskGroupTarget {
                    group_id,
                }),
                outcome,
            ),
            ClientContextMenuAction::ToggleGroup => {
                let endpoint_id = self.active_endpoint_id.clone();
                self.toggle_collapsed_group(&endpoint_id, collapse_key(&group_id));
                self.persist_chrome_preferences(outcome);
            }
            _ => {}
        }
    }
}
