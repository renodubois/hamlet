use super::*;

#[gpui_kit::test]
fn live_rename_duplicates_unknown_and_obsolete_deliveries(cx: &mut TestAppContext) {
    let (activity, calls, _, stream) = ready(cx);
    activity.edit_draft("keep".into());
    activity.rename_channel("2", "AAA");
    drain(cx, &activity);
    let call = calls.try_recv().unwrap();
    let rename = |id: &str, name: &str| serde_json::json!({"type":"channel_renamed","channel":{"id":id,"name":name,"type":"text"}});
    for _ in 0..2 {
        stream.frame("change", rename("2", "AAA"));
        drain(cx, &activity);
    }
    stream.frame("change", rename("99", "Unknown"));
    drain(cx, &activity);
    assert!(activity.read().rename_pending);
    assert_eq!(activity.read().rename_confirmed, 0);
    assert_eq!(activity.read().selected.as_deref(), Some("1"));
    assert_eq!(activity.read().draft("1"), "keep");
    assert!(
        matches!(&activity.read().channels, Some(Load::Ready(channels)) if channels.len() == 2 && channels[0].id == "2")
    );
    assert!(calls.is_empty());
    respond(
        call,
        serde_json::json!({"id":"2","name":"AAA","type":"text"}),
    );
    drain(cx, &activity);
    assert_eq!(activity.read().rename_confirmed, 1);
    stream.frame("change", rename("1", "Old session"));
    cx.executor().run_until_parked();
    let late = activity.updates().try_recv().unwrap();
    activity.close();
    let (new, new_calls, _, new_stream) = ready(cx);
    assert!(activity.apply(late).is_none());
    assert_eq!(new.read().selected_channel().unwrap().name, "General");
    // Abandoned attempt delivery cannot alter this workspace after failure.
    new_stream.frame("change", rename("1", "Obsolete"));
    cx.executor().run_until_parked();
    let obsolete = new.updates().try_recv().unwrap();
    new_stream.body.close();
    drain(cx, &new);
    assert!(new.apply(obsolete).is_none());
    assert_eq!(new.read().selected_channel().unwrap().name, "General");
    assert!(new_calls.is_empty());
    new.close();
}

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
