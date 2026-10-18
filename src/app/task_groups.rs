//! Pure task-group mutations; no PTYs, Git operations, or filesystem side effects.

use std::collections::HashSet;

use crate::task_groups::{normalized_label, TaskGroup};

use super::AppState;

impl AppState {
    pub(crate) fn workspace_task_group(&self, workspace_id: &str) -> Option<&TaskGroup> {
        self.task_groups.iter().find(|group| {
            group
                .workspace_ids
                .iter()
                .any(|member| member == workspace_id)
        })
    }

    pub(crate) fn create_task_group(&mut self, label: String) -> Result<TaskGroup, String> {
        let label = normalized_label(label)?;
        let id = loop {
            let candidate = crate::task_groups::new_id();
            if !self.task_groups.iter().any(|group| group.id == candidate) {
                break candidate;
            }
        };
        let group = TaskGroup {
            id,
            label,
            workspace_ids: Vec::new(),
        };
        self.task_groups.push(group.clone());
        self.mark_session_dirty();
        Ok(group)
    }

    pub(crate) fn rename_task_group(
        &mut self,
        group_id: &str,
        label: String,
    ) -> Result<TaskGroup, String> {
        let label = normalized_label(label)?;
        let group = self
            .task_groups
            .iter_mut()
            .find(|group| group.id == group_id)
            .ok_or_else(|| format!("task group {group_id} not found"))?;
        let changed = group.label != label;
        group.label = label;
        let result = group.clone();
        if changed {
            self.mark_session_dirty();
        }
        Ok(result)
    }

    /// Delete organization only. Workspaces, terminals, worktrees, and branches survive.
    pub(crate) fn delete_task_group(&mut self, group_id: &str) -> Result<(), String> {
        let index = self
            .task_groups
            .iter()
            .position(|group| group.id == group_id)
            .ok_or_else(|| format!("task group {group_id} not found"))?;
        self.task_groups.remove(index);
        self.mark_session_dirty();
        Ok(())
    }

    /// Assign or move exactly one workspace. None returns it to the ungrouped list.
    pub(crate) fn assign_workspace_task_group(
        &mut self,
        workspace_id: &str,
        group_id: Option<&str>,
    ) -> Result<(), String> {
        if !self
            .workspaces
            .iter()
            .any(|workspace| workspace.id == workspace_id)
        {
            return Err(format!("workspace {workspace_id} not found"));
        }
        if let Some(group_id) = group_id {
            if !self.task_groups.iter().any(|group| group.id == group_id) {
                return Err(format!("task group {group_id} not found"));
            }
        }
        if self
            .workspace_task_group(workspace_id)
            .map(|group| group.id.as_str())
            == group_id
        {
            return Ok(());
        }
        for group in &mut self.task_groups {
            group.workspace_ids.retain(|member| member != workspace_id);
            if group_id == Some(group.id.as_str()) {
                group.workspace_ids.push(workspace_id.to_owned());
            }
        }
        self.mark_session_dirty();
        Ok(())
    }

    /// Recover a safe deterministic membership set after restore or workspace closure.
    /// Unknown members are dropped; first valid membership wins; empty groups survive.
    pub(crate) fn reconcile_task_groups(&mut self) {
        let workspace_ids = self
            .workspaces
            .iter()
            .map(|ws| ws.id.as_str())
            .collect::<HashSet<_>>();
        let mut group_ids = HashSet::new();
        let mut assigned = HashSet::new();
        self.task_groups.retain_mut(|group| {
            if group.id.is_empty()
                || group.id.chars().any(char::is_control)
                || !group_ids.insert(group.id.clone())
            {
                return false;
            }
            let Ok(label) = normalized_label(group.label.clone()) else {
                return false;
            };
            group.label = label;
            group
                .workspace_ids
                .retain(|id| workspace_ids.contains(id.as_str()) && assigned.insert(id.clone()));
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{Workspace, WorktreeSpaceMembership};
    use std::path::PathBuf;

    fn state() -> AppState {
        let mut state = AppState::test_new();
        for (index, (repo, task)) in [
            ("bots", "taskA"),
            ("common", "taskA"),
            ("bots", "taskB"),
            ("common", "taskB"),
        ]
        .iter()
        .enumerate()
        {
            let mut workspace = Workspace::test_new(&format!("{repo}/{task}"));
            workspace.id = format!("w{}", index + 1);
            workspace.identity_cwd = PathBuf::from(format!("/repos/{repo}/{task}"));
            workspace.worktree_space = Some(WorktreeSpaceMembership {
                key: format!("/repos/{repo}/.git"),
                label: (*repo).into(),
                repo_root: PathBuf::from(format!("/repos/{repo}")),
                checkout_path: workspace.identity_cwd.clone(),
                is_linked_worktree: true,
            });
            state.workspaces.push(workspace);
        }
        state.active = Some(0);
        state.ensure_test_terminals();
        state
    }

    #[test]
    fn task_groups_partition_individual_workspaces_across_repositories() {
        let mut state = state();
        let a = state.create_task_group("taskA".into()).unwrap();
        let b = state.create_task_group("taskB".into()).unwrap();
        for id in ["w1", "w2"] {
            state.assign_workspace_task_group(id, Some(&a.id)).unwrap();
        }
        for id in ["w3", "w4"] {
            state.assign_workspace_task_group(id, Some(&b.id)).unwrap();
        }
        assert_eq!(state.task_groups[0].workspace_ids, ["w1", "w2"]);
        assert_eq!(state.task_groups[1].workspace_ids, ["w3", "w4"]);
        state.assert_invariants_for_test();
    }

    #[test]
    fn task_group_move_unassign_and_delete_preserve_workspace_identity_and_git_metadata() {
        let mut state = state();
        let before = state
            .workspaces
            .iter()
            .map(|ws| (ws.id.clone(), ws.worktree_space.clone(), ws.tabs.len()))
            .collect::<Vec<_>>();
        let a = state.create_task_group("A".into()).unwrap();
        let b = state.create_task_group("B".into()).unwrap();
        state
            .assign_workspace_task_group("w1", Some(&a.id))
            .unwrap();
        state
            .assign_workspace_task_group("w2", Some(&a.id))
            .unwrap();
        state
            .assign_workspace_task_group("w1", Some(&b.id))
            .unwrap();
        assert_eq!(state.task_groups[0].workspace_ids, ["w2"]);
        assert_eq!(state.task_groups[1].workspace_ids, ["w1"]);
        state.assign_workspace_task_group("w1", None).unwrap();
        assert!(state.task_groups[1].workspace_ids.is_empty());
        state.delete_task_group(&a.id).unwrap();
        assert!(state.workspace_task_group("w2").is_none());
        assert_eq!(
            state
                .workspaces
                .iter()
                .map(|ws| (ws.id.clone(), ws.worktree_space.clone(), ws.tabs.len()))
                .collect::<Vec<_>>(),
            before
        );
        state.assert_invariants_for_test();
    }

    #[test]
    fn task_group_invalid_mutations_are_atomic_and_reassignment_is_idempotent() {
        let mut state = state();
        let a = state.create_task_group(" A ".into()).unwrap();
        assert_eq!(a.label, "A");
        state
            .assign_workspace_task_group("w1", Some(&a.id))
            .unwrap();
        let before = state.task_groups.clone();
        state.session_dirty = false;
        state
            .assign_workspace_task_group("w1", Some(&a.id))
            .unwrap();
        assert!(!state.session_dirty);
        assert!(state
            .assign_workspace_task_group("w1", Some("missing"))
            .is_err());
        assert!(state
            .assign_workspace_task_group("missing", Some(&a.id))
            .is_err());
        assert!(state.rename_task_group(&a.id, " \n ".into()).is_err());
        assert!(state.create_task_group("bad\nname".into()).is_err());
        assert!(state.delete_task_group("missing").is_err());
        assert_eq!(state.task_groups, before);
        assert!(!state.session_dirty);
        let renamed = state.rename_task_group(&a.id, " renamed ".into()).unwrap();
        assert_eq!(renamed.id, a.id);
        assert_eq!(renamed.label, "renamed");
        assert!(state.session_dirty);
    }

    #[test]
    fn task_group_restore_reconciles_orphans_duplicates_and_preserves_empty_groups() {
        let mut state = state();
        state.task_groups = vec![
            TaskGroup {
                id: "g1".into(),
                label: " A ".into(),
                workspace_ids: vec!["w1".into(), "w1".into(), "gone".into()],
            },
            TaskGroup {
                id: "g2".into(),
                label: "B".into(),
                workspace_ids: vec!["w1".into(), "w2".into()],
            },
            TaskGroup {
                id: "g2".into(),
                label: "duplicate".into(),
                workspace_ids: vec!["w3".into()],
            },
            TaskGroup {
                id: "g3".into(),
                label: "empty".into(),
                workspace_ids: vec![],
            },
        ];
        state.reconcile_task_groups();
        assert_eq!(state.task_groups.len(), 3);
        assert_eq!(state.task_groups[0].label, "A");
        assert_eq!(state.task_groups[0].workspace_ids, ["w1"]);
        assert_eq!(state.task_groups[1].workspace_ids, ["w2"]);
        assert!(state.task_groups[2].workspace_ids.is_empty());
        let new_group = state.create_task_group("new".into()).unwrap();
        assert!(!["g1", "g2", "g3"].contains(&new_group.id.as_str()));
    }

    #[test]
    fn task_groups_do_not_change_adversarial_workspace_or_pane_identity() {
        let mut state = AppState::test_with_adversarial_identity_state();
        let group = state.create_task_group("task".into()).unwrap();
        let ids = state
            .workspaces
            .iter()
            .map(|workspace| workspace.id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            state
                .assign_workspace_task_group(&id, Some(&group.id))
                .unwrap();
        }
        state.assert_invariants_for_test();
        state.delete_task_group(&group.id).unwrap();
        state.assert_invariants_for_test();
    }

    #[test]
    fn task_group_workspace_close_prunes_only_membership_and_retains_empty_group() {
        let mut state = state();
        let group = state.create_task_group("task".into()).unwrap();
        state
            .assign_workspace_task_group("w1", Some(&group.id))
            .unwrap();
        state.close_workspaces(vec![0]);
        assert_eq!(state.workspaces.len(), 3);
        assert_eq!(state.task_groups.len(), 1);
        assert!(state.task_groups[0].workspace_ids.is_empty());
        state.assert_invariants_for_test();
    }

    #[test]
    fn implicit_repository_close_does_not_cross_task_membership() {
        let mut state = state();
        state.workspaces[0]
            .worktree_space
            .as_mut()
            .unwrap()
            .is_linked_worktree = false;
        assert_eq!(state.workspace_close_indices(0), [0, 2]);
        let group = state.create_task_group("other task".into()).unwrap();
        state
            .assign_workspace_task_group("w3", Some(&group.id))
            .unwrap();
        assert_eq!(state.workspace_close_indices(0), [0]);
        state
            .assign_workspace_task_group("w1", Some(&group.id))
            .unwrap();
        assert_eq!(state.workspace_close_indices(0), [0]);
    }
    #[test]
    fn implicit_repository_close_is_single_workspace_when_any_task_boundary_exists() {
        let mut state = state();
        state.workspaces[0]
            .worktree_space
            .as_mut()
            .unwrap()
            .is_linked_worktree = false;
        let mut ungrouped = Workspace::test_new("bots/ungrouped");
        ungrouped.worktree_space = state.workspaces[2].worktree_space.clone();
        state.workspaces.push(ungrouped);
        state.ensure_test_terminals();
        assert_eq!(state.workspace_close_indices(0), [0, 2, 4]);
        let task = state.create_task_group("Task B".into()).unwrap();
        state
            .assign_workspace_task_group("w3", Some(&task.id))
            .unwrap();
        assert_eq!(state.workspace_close_indices(0), [0]);
        state.selected = 0;
        state.close_selected_workspace();
        assert_eq!(state.workspaces.len(), 4);
        assert_eq!(state.task_groups[0].workspace_ids, ["w3"]);
        state.assert_invariants_for_test();
    }
}
