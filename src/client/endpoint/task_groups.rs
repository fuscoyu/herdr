use crate::protocol::{endpoint::EndpointTaskGroups, ClientShellSnapshot};

/// Optional companions belong to one connection, endpoint boot, and snapshot revision.
/// Keeping this separate from the core snapshot preserves the generation-one codecs.
#[derive(Clone, Debug, Default)]
pub(crate) struct TaskGroupProjection {
    pending: Option<(u64, EndpointTaskGroups)>,
}

impl TaskGroupProjection {
    pub(crate) fn receive(
        &mut self,
        generation: u64,
        projection: EndpointTaskGroups,
        snapshot_generation: Option<u64>,
        snapshot: Option<&mut ClientShellSnapshot>,
    ) -> bool {
        if snapshot_generation.is_some_and(|current| current > generation)
            || self
                .pending
                .as_ref()
                .is_some_and(|(current, _)| *current > generation)
        {
            return false;
        }
        if snapshot_generation == Some(generation) {
            if let Some(snapshot) =
                snapshot.filter(|snapshot| snapshot.boot_id == projection.boot_id)
            {
                if snapshot.revision > projection.revision {
                    return false;
                }
                if snapshot.revision == projection.revision {
                    let changed = snapshot.task_groups != projection.task_groups;
                    snapshot.task_groups = projection.task_groups;
                    return changed;
                }
            }
        }
        if self.pending.as_ref().is_some_and(|(current, pending)| {
            *current == generation
                && pending.boot_id == projection.boot_id
                && pending.revision >= projection.revision
        }) {
            return false;
        }
        self.pending = Some((generation, projection));
        false
    }

    pub(crate) fn project(
        &mut self,
        generation: Option<u64>,
        snapshot: &mut ClientShellSnapshot,
        previous: Option<(&ClientShellSnapshot, Option<u64>)>,
    ) {
        let matches = self
            .pending
            .as_ref()
            .is_some_and(|(pending_generation, pending)| {
                generation == Some(*pending_generation)
                    && pending.boot_id == snapshot.boot_id
                    && pending.revision == snapshot.revision
            });
        if matches {
            if let Some((_, pending)) = self.pending.take() {
                snapshot.task_groups = pending.task_groups;
            }
        } else if let Some((previous, _)) = previous.filter(|(previous, previous_generation)| {
            *previous_generation == generation
                && previous.boot_id == snapshot.boot_id
                && previous.revision == snapshot.revision
        }) {
            // A duplicate core snapshot must not discard an already merged companion.
            snapshot.task_groups.clone_from(&previous.task_groups);
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|(pending_generation, pending)| {
                generation.is_none_or(|generation| {
                    *pending_generation < generation
                        || (*pending_generation == generation
                            && (pending.boot_id != snapshot.boot_id
                                || pending.revision <= snapshot.revision))
                })
            })
        {
            self.pending = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(boot_id: &str, revision: u64) -> ClientShellSnapshot {
        let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/endpoint-snapshot-v1.json"
        )))
        .unwrap();
        snapshot.boot_id = boot_id.into();
        snapshot.revision = revision;
        snapshot
    }

    fn projection(boot_id: &str, revision: u64) -> EndpointTaskGroups {
        EndpointTaskGroups {
            boot_id: boot_id.into(),
            revision,
            task_groups: vec![crate::task_groups::TaskGroup {
                id: "group-one".into(),
                label: "Ship feature".into(),
                workspace_ids: vec!["w1".into()],
            }],
        }
    }

    #[test]
    fn task_groups_companion_merges_only_into_matching_snapshot() {
        let mut state = TaskGroupProjection::default();
        state.receive(1, projection("boot", 4), None, None);
        let mut before = snapshot("boot", 3);
        state.project(Some(1), &mut before, None);
        assert!(before.task_groups.is_empty());
        let mut matching = snapshot("boot", 4);
        state.project(Some(1), &mut matching, Some((&before, Some(1))));
        assert_eq!(matching.task_groups, projection("boot", 4).task_groups);
        let mut duplicate = snapshot("boot", 4);
        state.project(Some(1), &mut duplicate, Some((&matching, Some(1))));
        assert_eq!(duplicate.task_groups, matching.task_groups);
        let mut next = snapshot("boot", 5);
        state.project(Some(1), &mut next, Some((&matching, Some(1))));
        assert!(next.task_groups.is_empty());
    }

    #[test]
    fn task_groups_companion_cannot_cross_boot_or_connection_generation() {
        for (generation, boot_id) in [(2, "boot"), (1, "restarted")] {
            let mut state = TaskGroupProjection::default();
            state.receive(1, projection("boot", 4), None, None);
            let mut current = snapshot(boot_id, 4);
            state.project(Some(generation), &mut current, None);
            assert!(current.task_groups.is_empty());
        }
        let mut state = TaskGroupProjection::default();
        let mut current = snapshot("boot", 4);
        assert!(!state.receive(1, projection("boot", 4), Some(2), Some(&mut current)));
        assert!(current.task_groups.is_empty());
    }

    #[test]
    fn task_groups_late_companion_applies_and_stale_revision_is_ignored() {
        let mut state = TaskGroupProjection::default();
        let mut current = snapshot("boot", 4);
        assert!(state.receive(2, projection("boot", 4), Some(2), Some(&mut current)));
        let expected = current.task_groups.clone();
        assert!(!state.receive(2, projection("boot", 3), Some(2), Some(&mut current)));
        assert_eq!(current.task_groups, expected);
        let mut cleared = projection("boot", 4);
        cleared.task_groups.clear();
        assert!(state.receive(2, cleared, Some(2), Some(&mut current)));
        assert!(current.task_groups.is_empty());
    }

    #[test]
    fn task_groups_absent_companion_keeps_legacy_snapshot_usable() {
        let mut state = TaskGroupProjection::default();
        let mut current = snapshot("old-server", 1);
        state.project(Some(1), &mut current, None);
        assert!(current.task_groups.is_empty());
        assert!(!current.workspaces.is_empty());
    }
}
