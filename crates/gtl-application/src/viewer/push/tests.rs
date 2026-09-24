use gtl_wire::viewer::push::ViewerPushRequest;

use super::*;

fn plan_at(name: &str) -> PushPlan {
    let commit: CommitId = "a".repeat(40).parse().unwrap();
    PushPlan {
        path: std::env::temp_dir().join(name).try_into().unwrap(),
        repository: PushRepository {
            branch: BranchName::try_new("feature").unwrap(),
            remote: RemoteName::try_new("origin").unwrap(),
            destination: GitRefName::try_new("refs/heads/main").unwrap(),
            url: RemoteUrl::try_new("/remote.git").unwrap(),
            head: commit.clone(),
            upstream: "b".repeat(40).parse().unwrap(),
        },
        commit,
        count: 2,
    }
}

fn status(operations: &ViewerPushOperations, id: ViewerPushId) -> ViewerPushStatus {
    get_viewer_push::execute(ViewerPushRequest { id }, operations).unwrap()
}

#[test]
fn confirmed_pushes_run_in_confirmation_order_per_repository() {
    let operations = ViewerPushOperations::default();
    let first = operations.insert(Ok(plan_at("push-test"))).unwrap();
    let later_review = operations.insert(Ok(plan_at("push-test"))).unwrap();
    let earlier_review = operations.insert(Ok(plan_at("push-test"))).unwrap();
    let independent = operations.insert(Ok(plan_at("other-push-test"))).unwrap();

    operations.queue(first).unwrap();
    operations.queue(earlier_review).unwrap();
    operations.queue(later_review).unwrap();
    operations.queue(independent).unwrap();
    operations.queue(first).unwrap();
    assert_eq!(status(&operations, later_review), ViewerPushStatus::Queued);
    assert_eq!(operations.begin_next().unwrap().unwrap().0, first);
    assert_eq!(operations.begin_next().unwrap().unwrap().0, independent);
    assert!(operations.begin_next().unwrap().is_none());

    operations.finish(first, Ok(())).unwrap();
    assert_eq!(status(&operations, first), ViewerPushStatus::Succeeded);
    assert_eq!(operations.begin_next().unwrap().unwrap().0, earlier_review);
    operations.finish(earlier_review, Ok(())).unwrap();
    assert_eq!(operations.begin_next().unwrap().unwrap().0, later_review);
    operations.finish(later_review, Ok(())).unwrap();
    operations.queue(first).unwrap();
    assert!(operations.begin_next().unwrap().is_none());
}

#[test]
fn confirmed_pushes_do_not_expire_while_waiting() {
    let operations = ViewerPushOperations::default();
    let expired_review = operations.insert(Ok(plan_at("push-test"))).unwrap();
    operations.0.lock().unwrap().history[0].created -= REVIEW_LIFETIME + Duration::from_secs(1);
    operations.queue(expired_review).unwrap();
    assert!(matches!(
        status(&operations, expired_review),
        ViewerPushStatus::Failed { .. }
    ));

    let confirmed = operations.insert(Ok(plan_at("push-test"))).unwrap();
    operations.queue(confirmed).unwrap();
    operations.0.lock().unwrap().history[1].created -= REVIEW_LIFETIME + Duration::from_secs(1);
    assert_eq!(operations.begin_next().unwrap().unwrap().0, confirmed);
}

#[test]
fn capacity_eviction_preserves_queued_and_running_pushes() {
    let operations = ViewerPushOperations::default();
    let running = operations.insert(Ok(plan_at("push-test"))).unwrap();
    operations.queue(running).unwrap();
    assert_eq!(operations.begin_next().unwrap().unwrap().0, running);
    let queued = operations.insert(Ok(plan_at("push-test"))).unwrap();
    operations.queue(queued).unwrap();
    for _ in 0..OPERATIONS_MAX * 2 {
        operations.insert(Ok(plan_at("push-test"))).unwrap();
    }
    assert_eq!(status(&operations, running), ViewerPushStatus::Running);
    assert_eq!(status(&operations, queued), ViewerPushStatus::Queued);

    let full = ViewerPushOperations::default();
    for _ in 0..OPERATIONS_MAX {
        let id = full.insert(Ok(plan_at("push-test"))).unwrap();
        full.queue(id).unwrap();
    }
    assert!(matches!(
        full.insert(Ok(plan_at("push-test"))),
        Err(PushError::Refused(PushFailure::HistoryFull {
            operations_max: OPERATIONS_MAX
        }))
    ));
}
