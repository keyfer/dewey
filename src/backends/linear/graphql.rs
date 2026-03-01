use serde_json::{json, Map, Value};

/// Fragment containing the issue fields we request from Linear.
const ISSUE_FIELDS: &str = "
    id
    identifier
    title
    description
    priority
    priorityLabel
    dueDate
    createdAt
    completedAt
    url
    branchName
    state { name type }
    labels { nodes { name } }
    project { name }
    assignee { id name }
";

/// Build a query to fetch issues assigned to a user on a team, filtered by workflow state names.
///
/// - `team_id`: the Linear team UUID
/// - `assignee_id`: the Linear user UUID (the assignee)
/// - `statuses`: workflow state names to include (e.g. `["In Progress", "Todo"]`)
pub fn issues_query(team_id: &str, assignee_id: &str, statuses: &[String]) -> Value {
    let query = format!(
        r#"query AssignedIssues($teamId: ID!, $assigneeId: ID!, $statuses: [String!]!) {{
  issues(
    filter: {{
      team: {{ id: {{ eq: $teamId }} }}
      assignee: {{ id: {{ eq: $assigneeId }} }}
      state: {{ name: {{ in: $statuses }} }}
    }}
    orderBy: updatedAt
    first: 100
  ) {{
    nodes {{
      {ISSUE_FIELDS}
    }}
  }}
}}"#
    );

    json!({
        "query": query,
        "variables": {
            "teamId": team_id,
            "assigneeId": assignee_id,
            "statuses": statuses,
        }
    })
}

/// Build a mutation to create a new issue.
///
/// - `team_id`: the Linear team UUID
/// - `title`: issue title
/// - `priority`: Linear priority integer (0=None, 1=Urgent, 2=High, 3=Normal, 4=Low)
/// - `due_date`: optional ISO-8601 date string (e.g. `"2026-03-01"`)
/// - `assignee_id`: optional user UUID to assign the issue to
pub fn create_issue_mutation(
    team_id: &str,
    title: &str,
    priority: i32,
    due_date: Option<&str>,
    assignee_id: Option<&str>,
    label_ids: &[String],
    project_id: Option<&str>,
) -> Value {
    let query = format!(
        r#"mutation CreateIssue($teamId: String!, $title: String!, $priority: Int!, $dueDate: TimelessDate, $assigneeId: String, $labelIds: [String!], $projectId: String) {{
  issueCreate(input: {{
    teamId: $teamId
    title: $title
    priority: $priority
    dueDate: $dueDate
    assigneeId: $assigneeId
    labelIds: $labelIds
    projectId: $projectId
  }}) {{
    success
    issue {{
      {ISSUE_FIELDS}
    }}
  }}
}}"#
    );

    json!({
        "query": query,
        "variables": {
            "teamId": team_id,
            "title": title,
            "priority": priority,
            "dueDate": due_date,
            "assigneeId": assignee_id,
            "labelIds": if label_ids.is_empty() { Value::Null } else { json!(label_ids) },
            "projectId": project_id,
        }
    })
}

/// Build a mutation to update an existing issue.
///
/// - `issue_id`: the Linear issue UUID
/// - `updates`: a map of field names to new values (e.g. `{"title": "New title", "priority": 2}`)
pub fn update_issue_mutation(issue_id: &str, updates: &Map<String, Value>) -> Value {
    // Build the input fields dynamically from the updates map.
    // We construct the variable declarations and pass the updates as the input object.
    let query = format!(
        r#"mutation UpdateIssue($issueId: String!, $input: IssueUpdateInput!) {{
  issueUpdate(id: $issueId, input: $input) {{
    success
    issue {{
      {ISSUE_FIELDS}
    }}
  }}
}}"#
    );

    json!({
        "query": query,
        "variables": {
            "issueId": issue_id,
            "input": updates,
        }
    })
}

/// Build a mutation to archive (soft-delete) an issue.
///
/// - `issue_id`: the Linear issue UUID
pub fn archive_issue_mutation(issue_id: &str) -> Value {
    let query = format!(
        r#"mutation ArchiveIssue($issueId: String!) {{
  issueArchive(id: $issueId) {{
    success
    entity {{
      {ISSUE_FIELDS}
    }}
  }}
}}"#
    );

    json!({
        "query": query,
        "variables": {
            "issueId": issue_id,
        }
    })
}

/// Build a query to fetch all teams the authenticated user has access to.
pub fn teams_query() -> Value {
    let query = r#"query Teams {
  teams {
    nodes {
      id
      name
      key
    }
  }
}"#;

    json!({
        "query": query,
        "variables": {}
    })
}

/// Build a query to fetch the authenticated user's info.
pub fn viewer_query() -> Value {
    let query = r#"query Viewer {
  viewer {
    id
    name
    displayName
  }
}"#;

    json!({
        "query": query,
        "variables": {}
    })
}

/// Build a query to fetch all workflow states for a team.
///
/// - `team_id`: the Linear team UUID
pub fn workflow_states_query(team_id: &str) -> Value {
    let query = r#"query WorkflowStates($teamId: ID!) {
  workflowStates(filter: { team: { id: { eq: $teamId } } }) {
    nodes {
      id
      name
      type
      position
    }
  }
}"#;

    json!({
        "query": query,
        "variables": {
            "teamId": team_id,
        }
    })
}

/// Build a query to fetch all labels for a team.
///
/// - `team_id`: the Linear team UUID
pub fn team_labels_query(team_id: &str) -> Value {
    let query = r#"query TeamLabels($teamId: ID!) {
  issueLabels(filter: { team: { id: { eq: $teamId } } }, first: 250) {
    nodes {
      id
      name
    }
  }
}"#;

    json!({
        "query": query,
        "variables": {
            "teamId": team_id,
        }
    })
}

/// Build a query to fetch all projects associated with a team.
///
/// - `team_id`: the Linear team UUID
pub fn team_projects_query(team_id: &str) -> Value {
    let query = r#"query TeamProjects($teamId: String!) {
  team(id: $teamId) {
    projects {
      nodes {
        id
        name
      }
    }
  }
}"#;

    json!({
        "query": query,
        "variables": {
            "teamId": team_id,
        }
    })
}

/// Build a query to fetch members of a team.
///
/// - `team_id`: the Linear team UUID
pub fn team_members_query(team_id: &str) -> Value {
    let query = r#"query TeamMembers($teamId: String!) {
  team(id: $teamId) {
    members {
      nodes {
        id
        name
        displayName
      }
    }
  }
}"#;

    json!({
        "query": query,
        "variables": {
            "teamId": team_id,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issues_query_has_correct_structure() {
        let result = issues_query(
            "team-123",
            "user-456",
            &["In Progress".to_string(), "Todo".to_string()],
        );

        let query = result["query"].as_str().unwrap();
        assert!(query.contains("query AssignedIssues"));
        assert!(query.contains("$teamId"));
        assert!(query.contains("$assigneeId"));
        assert!(query.contains("$statuses"));
        assert!(query.contains("identifier"));
        assert!(query.contains("state { name type }"));
        assert!(query.contains("labels { nodes { name } }"));
        assert!(query.contains("project { name }"));
        assert!(query.contains("assignee { id name }"));
        assert!(query.contains("branchName"));

        let vars = &result["variables"];
        assert_eq!(vars["teamId"], "team-123");
        assert_eq!(vars["assigneeId"], "user-456");
        assert_eq!(vars["statuses"][0], "In Progress");
        assert_eq!(vars["statuses"][1], "Todo");
    }

    #[test]
    fn create_issue_mutation_with_due_date() {
        let result = create_issue_mutation("team-123", "Fix the bug", 2, Some("2026-03-01"), Some("user-456"), &[], None);

        let query = result["query"].as_str().unwrap();
        assert!(query.contains("mutation CreateIssue"));
        assert!(query.contains("issueCreate"));
        assert!(query.contains("$dueDate"));
        assert!(query.contains("$labelIds"));
        assert!(query.contains("$projectId"));
        assert!(query.contains("success"));

        let vars = &result["variables"];
        assert_eq!(vars["teamId"], "team-123");
        assert_eq!(vars["title"], "Fix the bug");
        assert_eq!(vars["priority"], 2);
        assert_eq!(vars["dueDate"], "2026-03-01");
    }

    #[test]
    fn create_issue_mutation_without_due_date() {
        let result = create_issue_mutation("team-123", "No deadline task", 4, None, None, &[], None);

        let vars = &result["variables"];
        assert_eq!(vars["title"], "No deadline task");
        assert!(vars["dueDate"].is_null());
    }

    #[test]
    fn create_issue_mutation_with_labels_and_project() {
        let labels = vec!["label-id-1".to_string(), "label-id-2".to_string()];
        let result = create_issue_mutation("team-123", "Labeled task", 0, None, None, &labels, Some("project-id"));

        let vars = &result["variables"];
        assert_eq!(vars["labelIds"][0], "label-id-1");
        assert_eq!(vars["labelIds"][1], "label-id-2");
        assert_eq!(vars["projectId"], "project-id");
    }

    #[test]
    fn update_issue_mutation_with_fields() {
        let mut updates = Map::new();
        updates.insert("title".to_string(), json!("Updated title"));
        updates.insert("priority".to_string(), json!(1));

        let result = update_issue_mutation("issue-789", &updates);

        let query = result["query"].as_str().unwrap();
        assert!(query.contains("mutation UpdateIssue"));
        assert!(query.contains("issueUpdate"));
        assert!(query.contains("$input: IssueUpdateInput!"));

        let vars = &result["variables"];
        assert_eq!(vars["issueId"], "issue-789");
        assert_eq!(vars["input"]["title"], "Updated title");
        assert_eq!(vars["input"]["priority"], 1);
    }

    #[test]
    fn archive_issue_mutation_structure() {
        let result = archive_issue_mutation("issue-abc");

        let query = result["query"].as_str().unwrap();
        assert!(query.contains("mutation ArchiveIssue"));
        assert!(query.contains("issueArchive"));
        assert!(query.contains("success"));

        let vars = &result["variables"];
        assert_eq!(vars["issueId"], "issue-abc");
    }

    #[test]
    fn teams_query_structure() {
        let result = teams_query();

        let query = result["query"].as_str().unwrap();
        assert!(query.contains("query Teams"));
        assert!(query.contains("teams"));
        assert!(query.contains("id"));
        assert!(query.contains("name"));
        assert!(query.contains("key"));

        // No variables needed
        assert!(result["variables"].as_object().unwrap().is_empty());
    }

    #[test]
    fn viewer_query_structure() {
        let result = viewer_query();

        let query = result["query"].as_str().unwrap();
        assert!(query.contains("query Viewer"));
        assert!(query.contains("viewer"));
        assert!(query.contains("id"));
        assert!(query.contains("name"));
        assert!(query.contains("displayName"));

        assert!(result["variables"].as_object().unwrap().is_empty());
    }

    #[test]
    fn workflow_states_query_structure() {
        let result = workflow_states_query("team-123");

        let query = result["query"].as_str().unwrap();
        assert!(query.contains("query WorkflowStates"));
        assert!(query.contains("workflowStates"));
        assert!(query.contains("name"));
        assert!(query.contains("type"));
        assert!(query.contains("position"));

        let vars = &result["variables"];
        assert_eq!(vars["teamId"], "team-123");
    }

    #[test]
    fn team_members_query_structure() {
        let result = team_members_query("team-123");

        let query = result["query"].as_str().unwrap();
        assert!(query.contains("query TeamMembers"));
        assert!(query.contains("members"));
        assert!(query.contains("id"));
        assert!(query.contains("name"));
        assert!(query.contains("displayName"));

        let vars = &result["variables"];
        assert_eq!(vars["teamId"], "team-123");
    }

    #[test]
    fn team_labels_query_structure() {
        let result = team_labels_query("team-123");

        let query = result["query"].as_str().unwrap();
        assert!(query.contains("issueLabels"));
        assert!(query.contains("id"));
        assert!(query.contains("name"));

        let vars = &result["variables"];
        assert_eq!(vars["teamId"], "team-123");
    }

    #[test]
    fn team_projects_query_structure() {
        let result = team_projects_query("team-123");

        let query = result["query"].as_str().unwrap();
        assert!(query.contains("projects"));
        assert!(query.contains("id"));
        assert!(query.contains("name"));

        let vars = &result["variables"];
        assert_eq!(vars["teamId"], "team-123");
    }

    #[test]
    fn issues_query_requests_all_required_fields() {
        let result = issues_query("t", "u", &[]);
        let query = result["query"].as_str().unwrap();

        for field in &[
            "id",
            "identifier",
            "title",
            "description",
            "priority",
            "priorityLabel",
            "dueDate",
            "createdAt",
            "completedAt",
            "url",
            "branchName",
        ] {
            assert!(
                query.contains(field),
                "issues_query is missing field: {field}"
            );
        }
    }

    #[test]
    fn all_queries_produce_valid_json() {
        // Ensure every builder returns a Value with "query" and "variables" keys.
        let queries: Vec<Value> = vec![
            issues_query("t", "u", &["s".to_string()]),
            create_issue_mutation("t", "title", 0, None, None, &[], None),
            update_issue_mutation("i", &Map::new()),
            archive_issue_mutation("i"),
            teams_query(),
            viewer_query(),
            workflow_states_query("t"),
            team_labels_query("t"),
            team_projects_query("t"),
            team_members_query("t"),
        ];

        for (idx, q) in queries.iter().enumerate() {
            assert!(
                q.get("query").is_some(),
                "query #{idx} is missing 'query' key"
            );
            assert!(
                q.get("variables").is_some(),
                "query #{idx} is missing 'variables' key"
            );
            assert!(
                q["query"].is_string(),
                "query #{idx} 'query' is not a string"
            );
            assert!(
                q["variables"].is_object(),
                "query #{idx} 'variables' is not an object"
            );
        }
    }
}
