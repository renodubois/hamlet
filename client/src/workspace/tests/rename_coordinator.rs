use super::*;

#[gpui_kit::test]
fn rename_queued_old_results_cannot_affect_new_authentication(cx: &mut TestAppContext) {
    for rejected in [false, true] {
        let (old, calls, _, _) = ready(cx);
        old.rename_channel("2", "New");
        drain(cx, &old);
        let call = calls.try_recv().unwrap();
        call.reply
            .try_send(Ok(Response::controlled(
                if rejected {
                    StatusCode::UNAUTHORIZED
                } else {
                    StatusCode::OK
                },
                if rejected {
                    ""
                } else {
                    r#"{"id":"2","name":"New","type":"text"}"#
                },
            )))
            .unwrap();
        cx.executor().run_until_parked();
        let late = old.updates().try_recv().unwrap();
        old.close();
        let (new, new_calls, _, _) = ready(cx);
        assert!(old.apply(late).is_none());
        assert!(old.read().channels.is_none());
        assert!(!new.read().rename_pending);
        assert!(new.read().rename_feedback.is_none());
        assert_eq!(new.read().rename_confirmed, 0);
        assert!(new_calls.is_empty());
        new.close();
    }
}

#[gpui_kit::test]
fn rename_current_authoritative_rejection_closes_workspace(cx: &mut TestAppContext) {
    let (activity, calls, _, _) = ready(cx);
    activity.rename_channel("1", "New");
    drain(cx, &activity);
    calls
        .try_recv()
        .unwrap()
        .reply
        .try_send(Ok(Response::controlled(StatusCode::UNAUTHORIZED, "")))
        .unwrap();
    cx.executor().run_until_parked();
    let result = activity.apply(activity.updates().try_recv().unwrap());
    assert!(matches!(result, Some(SessionEnd::Rejected(_))));
    assert!(activity.read().channels.is_none());
}
