use codegraph::config::Config;
use codegraph::session::{SessionManager, TaskStatus};
use codegraph::store::{CodeGraph, Store};
use std::sync::{Arc, RwLock};
use tempfile::TempDir;

async fn setup_session() -> (Arc<SessionManager>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path();

    let config = Config {
        project_root: temp_path.to_path_buf(),
        cache_dir: temp_path.join("cache"),
        codegraph_dir: temp_path.join(".codegraph"),
        store_db_path: temp_path.join("cache/store.db"),
        learning_db_path: temp_path.join(".codegraph/learning.db"),
        settings: codegraph::config::ConfigFile::default(),
    };
    config.ensure_dirs().unwrap();

    let store = Arc::new(Store::open(&config).await.unwrap());
    let graph = Arc::new(RwLock::new(
        CodeGraph::load_from_store(&store).await.unwrap(),
    ));

    let manager = Arc::new(SessionManager::new(store, graph));
    (manager, temp_dir)
}

#[tokio::test]
async fn test_session_full_lifecycle() {
    let (manager, _temp) = setup_session().await;

    // 1. Start session with items
    let session = manager
        .start_session(
            "Implement user auth",
            &[
                "Design API".to_string(),
                "Write tests".to_string(),
                "Implement handler".to_string(),
            ],
        )
        .await
        .unwrap();

    assert_eq!(session.task, "Implement user auth");
    assert_eq!(session.items.len(), 3);

    // 2. Update first item to in_progress
    let item_id = session.items[0].id.clone();
    let session = manager
        .update_task(
            Some(&item_id),
            Some(TaskStatus::InProgress),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(session.items[0].status, TaskStatus::InProgress);

    // 3. Add a decision
    manager
        .add_decision(
            "Use JWT for auth tokens",
            "Stateless, works well with microservices",
            &[],
        )
        .await
        .unwrap();

    // 4. Update context
    manager
        .set_context(
            Some("src/auth.rs"),
            None,
            Some("AuthHandler"),
            None,
            None,
        )
        .await
        .unwrap();

    // 5. Get smart context
    let ctx = manager.smart_context().await.unwrap();
    assert_eq!(ctx.task, "Implement user auth");
    assert_eq!(ctx.progress, "0/3 tasks completed");
    assert_eq!(ctx.recent_decisions.len(), 1);
    assert_eq!(ctx.recent_decisions[0].what, "Use JWT for auth tokens");
    assert!(ctx.files_modified.contains(&"src/auth.rs".to_string()));
    assert!(ctx.working_symbols.contains(&"AuthHandler".to_string()));

    // 6. Complete the first item
    let session = manager
        .update_task(
            Some(&item_id),
            Some(TaskStatus::Completed),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(session.items[0].status, TaskStatus::Completed);

    // 7. Verify progress updated
    let ctx = manager.smart_context().await.unwrap();
    assert_eq!(ctx.progress, "1/3 tasks completed");

    // 8. Get full session
    let full = manager.get_session().await.unwrap().unwrap();
    assert_eq!(full.decisions.len(), 1);
    assert_eq!(full.items.len(), 3);
}

#[tokio::test]
async fn test_session_add_items_dynamically() {
    let (manager, _temp) = setup_session().await;

    manager
        .start_session("Build feature", &["Step 1".to_string()])
        .await
        .unwrap();

    // Add a new item dynamically
    let session = manager
        .update_task(None, None, Some("Step 2 - discovered during work"), None, None)
        .await
        .unwrap();

    assert_eq!(session.items.len(), 2);
}
