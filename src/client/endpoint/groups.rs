use crate::protocol::{endpoint::EndpointGroups, ClientShellSnapshot};

/// Optional companions belong to one connection, endpoint boot, and snapshot revision.
/// Keeping this separate from the core snapshot preserves the generation-one codecs.
#[derive(Clone, Debug, Default)]
pub(crate) struct GroupProjection {
    pending: Option<(u64, EndpointGroups)>,
}

impl GroupProjection {
    pub(crate) fn receive(
        &mut self,
        generation: u64,
        projection: EndpointGroups,
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
                    let changed = snapshot.groups != projection.groups;
                    snapshot.groups = projection.groups;
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
                snapshot.groups = pending.groups;
            }
        } else if let Some((previous, _)) = previous.filter(|(previous, previous_generation)| {
            *previous_generation == generation
                && previous.boot_id == snapshot.boot_id
                && previous.revision == snapshot.revision
        }) {
            // A duplicate core snapshot must not discard an already merged companion.
            snapshot.groups.clone_from(&previous.groups);
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

    fn projection(boot_id: &str, revision: u64) -> EndpointGroups {
        EndpointGroups {
            boot_id: boot_id.into(),
            revision,
            groups: vec![crate::groups::Group {
                id: "group-one".into(),
                label: "Ship feature".into(),
                workspace_ids: vec!["w1".into()],
            }],
        }
    }

    #[test]
    fn groups_companion_merges_only_into_matching_snapshot() {
        let mut state = GroupProjection::default();
        state.receive(1, projection("boot", 4), None, None);
        let mut before = snapshot("boot", 3);
        state.project(Some(1), &mut before, None);
        assert!(before.groups.is_empty());
        let mut matching = snapshot("boot", 4);
        state.project(Some(1), &mut matching, Some((&before, Some(1))));
        assert_eq!(matching.groups, projection("boot", 4).groups);
        let mut duplicate = snapshot("boot", 4);
        state.project(Some(1), &mut duplicate, Some((&matching, Some(1))));
        assert_eq!(duplicate.groups, matching.groups);
        let mut next = snapshot("boot", 5);
        state.project(Some(1), &mut next, Some((&matching, Some(1))));
        assert!(next.groups.is_empty());
    }

    #[test]
    fn groups_companion_cannot_cross_boot_or_connection_generation() {
        for (generation, boot_id) in [(2, "boot"), (1, "restarted")] {
            let mut state = GroupProjection::default();
            state.receive(1, projection("boot", 4), None, None);
            let mut current = snapshot(boot_id, 4);
            state.project(Some(generation), &mut current, None);
            assert!(current.groups.is_empty());
        }
        let mut state = GroupProjection::default();
        let mut current = snapshot("boot", 4);
        assert!(!state.receive(1, projection("boot", 4), Some(2), Some(&mut current)));
        assert!(current.groups.is_empty());
    }

    #[test]
    fn groups_late_companion_applies_and_stale_revision_is_ignored() {
        let mut state = GroupProjection::default();
        let mut current = snapshot("boot", 4);
        assert!(state.receive(2, projection("boot", 4), Some(2), Some(&mut current)));
        let expected = current.groups.clone();
        assert!(!state.receive(2, projection("boot", 3), Some(2), Some(&mut current)));
        assert_eq!(current.groups, expected);
        let mut cleared = projection("boot", 4);
        cleared.groups.clear();
        assert!(state.receive(2, cleared, Some(2), Some(&mut current)));
        assert!(current.groups.is_empty());
    }

    #[test]
    fn groups_absent_companion_keeps_legacy_snapshot_usable() {
        let mut state = GroupProjection::default();
        let mut current = snapshot("old-server", 1);
        state.project(Some(1), &mut current, None);
        assert!(current.groups.is_empty());
        assert!(!current.workspaces.is_empty());
    }
}
