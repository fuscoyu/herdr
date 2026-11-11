use crate::api::schema::{
    GroupAssignParams, GroupCreateParams, GroupRenameParams, GroupTarget, ResponseResult,
};
use crate::app::App;

use super::responses::{encode_error, encode_success};

impl App {
    pub(super) fn handle_group_list(&self, id: String) -> String {
        encode_success(
            id,
            ResponseResult::GroupList {
                groups: self.state.groups.clone(),
            },
        )
    }

    pub(super) fn handle_group_create(&mut self, id: String, params: GroupCreateParams) -> String {
        // Resolve before creating anything so an invalid initial workspace cannot
        // leave behind an empty group. Membership always stores durable IDs.
        let workspace_id = match params.workspace_id {
            Some(workspace_id) => match self.parse_workspace_id(&workspace_id) {
                Some(index) => Some(self.state.workspaces[index].id.clone()),
                None => return workspace_not_found(id, &workspace_id),
            },
            None => None,
        };
        let was_dirty = self.state.session_dirty;
        match self.state.create_group(params.label) {
            Ok(mut group) => {
                if let Some(workspace_id) = workspace_id {
                    if let Err(error) = self
                        .state
                        .assign_workspace_group(&workspace_id, Some(&group.id))
                    {
                        // Both IDs were validated synchronously, but preserve the
                        // atomic create contract if assignment ever gains validation.
                        let _ = self.state.delete_group(&group.id);
                        self.state.session_dirty = was_dirty;
                        return encode_error(id, "group_assignment_failed", error);
                    }
                    group.workspace_ids.push(workspace_id);
                }
                if self.state.session_dirty {
                    self.schedule_session_save();
                }
                encode_success(id, ResponseResult::GroupInfo { group })
            }
            Err(error) => encode_error(id, "invalid_params", error),
        }
    }

    pub(super) fn handle_group_rename(&mut self, id: String, params: GroupRenameParams) -> String {
        if !self.has_group(&params.group_id) {
            return group_not_found(id, &params.group_id);
        }
        match self.state.rename_group(&params.group_id, params.label) {
            Ok(group) => {
                if self.state.session_dirty {
                    self.schedule_session_save();
                }
                encode_success(id, ResponseResult::GroupInfo { group })
            }
            Err(error) => encode_error(id, "invalid_params", error),
        }
    }

    pub(super) fn handle_group_delete(&mut self, id: String, params: GroupTarget) -> String {
        if !self.has_group(&params.group_id) {
            return group_not_found(id, &params.group_id);
        }
        match self.state.delete_group(&params.group_id) {
            Ok(()) => {
                if self.state.session_dirty {
                    self.schedule_session_save();
                }
                encode_success(id, ResponseResult::Ok {})
            }
            Err(error) => encode_error(id, "group_delete_failed", error),
        }
    }

    pub(super) fn handle_group_assign(&mut self, id: String, params: GroupAssignParams) -> String {
        let Some(index) = self.parse_workspace_id(&params.workspace_id) else {
            return workspace_not_found(id, &params.workspace_id);
        };
        if let Some(group_id) = params.group_id.as_deref() {
            if !self.has_group(group_id) {
                return group_not_found(id, group_id);
            }
        }
        let workspace_id = self.state.workspaces[index].id.clone();
        match self
            .state
            .assign_workspace_group(&workspace_id, params.group_id.as_deref())
        {
            Ok(()) => {
                if self.state.session_dirty {
                    self.schedule_session_save();
                }
                encode_success(id, ResponseResult::Ok {})
            }
            Err(error) => encode_error(id, "group_assignment_failed", error),
        }
    }

    fn has_group(&self, group_id: &str) -> bool {
        self.state.groups.iter().any(|group| group.id == group_id)
    }
}

fn group_not_found(id: String, group_id: &str) -> String {
    encode_error(id, "group_not_found", format!("group {group_id} not found"))
}

fn workspace_not_found(id: String, workspace_id: &str) -> String {
    encode_error(
        id,
        "workspace_not_found",
        format!("workspace {workspace_id} not found"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::schema::Request, config::Config, workspace::Workspace};

    fn app() -> App {
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        app
    }

    fn request(app: &mut App, method: &str, params: serde_json::Value) -> serde_json::Value {
        let request: Request = serde_json::from_value(serde_json::json!({
            "id": "group-test", "method": method, "params": params,
        }))
        .unwrap();
        let response = serde_json::from_str(&app.handle_api_request(request)).unwrap();
        app.state.assert_invariants_for_test();
        response
    }

    #[test]
    fn group_api_lifecycle_moves_only_the_selected_workspace() {
        let mut app = app();
        let workspace_ids: Vec<_> = app
            .state
            .workspaces
            .iter()
            .map(|ws| ws.id.clone())
            .collect();
        let created = request(
            &mut app,
            "group.create",
            serde_json::json!({
                "label": "Release", "workspace_id": "1",
            }),
        );
        let first = created["result"]["group"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            created["result"]["group"]["workspace_ids"],
            serde_json::json!([workspace_ids[0]])
        );
        assert!(app.state.session_dirty);
        let created = request(
            &mut app,
            "group.create",
            serde_json::json!({"label": "Review"}),
        );
        let second = created["result"]["group"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let assigned = request(
            &mut app,
            "group.assign",
            serde_json::json!({
                "workspace_id": workspace_ids[1], "group_id": first,
            }),
        );
        assert_eq!(assigned["result"]["type"], "ok");
        request(
            &mut app,
            "group.assign",
            serde_json::json!({
                "workspace_id": "w_1", "group_id": second,
            }),
        );
        assert_eq!(
            app.state.groups[0].workspace_ids,
            vec![workspace_ids[1].clone()]
        );
        assert_eq!(
            app.state.groups[1].workspace_ids,
            vec![workspace_ids[0].clone()]
        );
        let renamed = request(
            &mut app,
            "group.rename",
            serde_json::json!({
                "group_id": second, "label": "Final review",
            }),
        );
        assert_eq!(renamed["result"]["group"]["label"], "Final review");
        let listed = request(&mut app, "group.list", serde_json::json!({}));
        assert_eq!(listed["result"]["groups"].as_array().unwrap().len(), 2);
        request(
            &mut app,
            "group.assign",
            serde_json::json!({
                "workspace_id": workspace_ids[0], "group_id": null,
            }),
        );
        assert!(app.state.groups[1].workspace_ids.is_empty());
        request(
            &mut app,
            "group.delete",
            serde_json::json!({"group_id": first}),
        );
        assert_eq!(app.state.groups.len(), 1);
        assert_eq!(
            app.state
                .workspaces
                .iter()
                .map(|ws| ws.id.clone())
                .collect::<Vec<_>>(),
            workspace_ids
        );
    }

    #[test]
    fn group_api_invalid_requests_do_not_mutate_membership() {
        let mut app = app();
        let created = request(
            &mut app,
            "group.create",
            serde_json::json!({
                "label": "Original", "workspace_id": "1",
            }),
        );
        let group = created["result"]["group"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let before = app.state.groups.clone();
        app.state.session_dirty = false;
        for (method, params, code) in [
            (
                "group.create",
                serde_json::json!({"label":" \t"}),
                "invalid_params",
            ),
            (
                "group.create",
                serde_json::json!({"label":"line\nbreak"}),
                "invalid_params",
            ),
            (
                "group.create",
                serde_json::json!({"label":"x".repeat(129)}),
                "invalid_params",
            ),
            (
                "group.create",
                serde_json::json!({"label":"Invalid","workspace_id":"missing"}),
                "workspace_not_found",
            ),
            (
                "group.rename",
                serde_json::json!({"group_id":group,"label":" "}),
                "invalid_params",
            ),
            (
                "group.rename",
                serde_json::json!({"group_id":"missing","label":"New"}),
                "group_not_found",
            ),
            (
                "group.delete",
                serde_json::json!({"group_id":"missing"}),
                "group_not_found",
            ),
            (
                "group.assign",
                serde_json::json!({"group_id":"missing","workspace_id":"1"}),
                "group_not_found",
            ),
            (
                "group.assign",
                serde_json::json!({"group_id":group,"workspace_id":"missing"}),
                "workspace_not_found",
            ),
        ] {
            let response = request(&mut app, method, params);
            assert_eq!(response["error"]["code"], code, "{response}");
            assert_eq!(app.state.groups, before);
            assert!(!app.state.session_dirty);
        }
    }
}
