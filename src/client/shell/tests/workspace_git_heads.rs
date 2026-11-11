use super::*;
use crate::client::endpoint::{
    ClientEndpointId, ClientEndpointStatus, ProfileId, SavedSshEndpoint,
};

fn remote_profile() -> SavedSshEndpoint {
    SavedSshEndpoint {
        id: ProfileId::parse("0123456789abcdef0123456789abcdef").unwrap(),
        label: "Build".into(),
        target: "dev@build.example".into(),
        session: "agents".into(),
        enabled: true,
    }
}

#[test]
fn workspace_git_heads_companion_reaches_active_and_inactive_endpoint_snapshots() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let profile = remote_profile();
    let remote_id = ClientEndpointId::Ssh(profile.id.clone());
    state.set_endpoint_catalog(&[profile]);
    state.set_endpoint_status(&remote_id, ClientEndpointStatus::Online);
    let groups = vec!["ws_1".to_string()];
    let local = snapshot();
    assert!(!state.set_endpoint_workspace_git_heads(
        &ClientEndpointId::Local,
        1,
        crate::protocol::endpoint::EndpointWorkspaceGitHeads {
            boot_id: local.boot_id.clone(),
            revision: local.revision,
            detached_workspace_ids: groups.clone(),
        },
    ));
    state.set_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(local));
    assert_eq!(
        state.snapshot.as_ref().unwrap().detached_workspace_ids,
        groups
    );

    let remote = snapshot();
    state.cache_endpoint_snapshot_inactive_for_generation(&remote_id, 2, Box::new(remote.clone()));
    assert!(state.set_endpoint_workspace_git_heads(
        &remote_id,
        2,
        crate::protocol::endpoint::EndpointWorkspaceGitHeads {
            boot_id: remote.boot_id,
            revision: remote.revision,
            detached_workspace_ids: groups.clone(),
        },
    ));
    assert_eq!(
        state
            .endpoints
            .iter()
            .find(|endpoint| endpoint.endpoint_id == remote_id)
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .detached_workspace_ids,
        groups
    );
    assert_eq!(state.active_endpoint_id, ClientEndpointId::Local);
    state.retire_endpoint(&remote_id);
    assert!(state
        .endpoints
        .iter()
        .find(|endpoint| endpoint.endpoint_id == remote_id)
        .unwrap()
        .snapshot
        .is_none());
}

#[test]
fn late_workspace_git_heads_do_not_activate_a_snapshot_waiting_for_its_surface() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let first = snapshot();
    let mut next = first.clone();
    next.revision += 1;
    state.set_endpoint_snapshot_for_generation(
        &ClientEndpointId::Local,
        1,
        Box::new(first.clone()),
    );
    state.cache_endpoint_snapshot_inactive_for_generation(
        &ClientEndpointId::Local,
        1,
        Box::new(next.clone()),
    );
    state.set_endpoint_workspace_git_heads(
        &ClientEndpointId::Local,
        1,
        crate::protocol::endpoint::EndpointWorkspaceGitHeads {
            boot_id: next.boot_id,
            revision: next.revision,
            detached_workspace_ids: vec!["ws_1".into()],
        },
    );
    let presented = state.snapshot.as_ref().unwrap();
    assert_eq!(presented.revision, first.revision);
    assert!(presented.detached_workspace_ids.is_empty());
    assert_eq!(
        state.endpoints[0]
            .snapshot
            .as_ref()
            .unwrap()
            .detached_workspace_ids
            .len(),
        1
    );
}
