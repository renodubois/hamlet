use super::{ConversationHandle, Load, Older};
use crate::{api::HttpTransport, runtime::Execution};
use gpui_kit::TestAppContext;
use std::{net::TcpListener, time::Duration};

// The host only delivers opaque updates. All HTTP dispatch, completion policy and
// continuation/scheduling decisions stay in the production coordinator.
async fn await_conversation(
    cx: &mut TestAppContext,
    activity: &ConversationHandle,
    ready: impl Fn(&ConversationHandle) -> bool,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.executor().run_until_parked();
        while let Ok(update) = activity.updates().try_recv() {
            assert!(activity.apply(update).is_none());
        }
        if ready(activity) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "conversation workflow did not complete"
        );
        // Yield only for real loopback I/O; application time advances explicitly.
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

#[gpui_kit::test]
fn second_user_activity_is_found_by_focused_polling_against_unchanged_routes(
    cx: &mut TestAppContext,
) {
    actix_web::rt::System::new().block_on(async {
        use actix_web::{App, HttpServer, web};
        let dir = tempfile::tempdir().unwrap();
        let db = hamlet::connect_to_database(&format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("polling.db").display()
        ))
        .await
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(db.clone()))
                .configure(hamlet::routes)
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        let api = HttpTransport::new().server(&url).unwrap();
        let alice = api
            .signup("Alice".into(), "long password".into())
            .await
            .unwrap();
        let bob = api
            .signup("Bob".into(), "long password".into())
            .await
            .unwrap();
        let channel = alice.client.channels().await.unwrap()[0].id.clone();
        let activity = ConversationHandle::new(
            1,
            alice.expires_at,
            alice.client,
            Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600),
        );
        activity.start(true);
        await_conversation(cx, &activity, |activity| {
            matches!(
                activity.read().history.get(&channel),
                Some(Load::Ready(messages)) if messages.is_empty()
            )
        })
        .await;
        let posted = bob
            .client
            .send_message(
                channel.clone(),
                "Bob published while Alice was reading".into(),
            )
            .await
            .unwrap();
        let new_channel = bob.client.create_channel("Bob room".into()).await.unwrap();
        cx.background_executor.advance_clock(Duration::from_secs(3));
        await_conversation(cx, &activity, |activity| {
            matches!(
                activity.read().history.get(&channel),
                Some(Load::Ready(messages)) if messages.iter().any(|m| m.id == posted.id && m.author_name == "Bob")
            )
        })
        .await;
        assert!(
            matches!(
                activity.read().channels,
                Some(Load::Ready(ref channels)) if !channels.contains(&new_channel)
            ),
            "channel discovery waits for its independent fifteen-second poll"
        );
        cx.background_executor.advance_clock(Duration::from_secs(12));
        await_conversation(cx, &activity, |activity| {
            matches!(
                activity.read().channels,
                Some(Load::Ready(ref channels)) if channels.contains(&new_channel)
            )
        })
        .await;
        activity.close();
        handle.stop(true).await;
    });
}

#[gpui_kit::test]
fn rewrite_history_traverses_multiple_pages_with_timestamp_ties(cx: &mut TestAppContext) {
    actix_web::rt::System::new().block_on(async {
        use actix_web::{App, HttpServer, web};
        use hamlet::{connect_to_database, routes};
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let dir = tempfile::tempdir().unwrap();
        let db = connect_to_database(&format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("pages.db").display()
        ))
        .await
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new({
            let db = db.clone();
            move || {
                App::new()
                    .app_data(web::Data::new(db.clone()))
                    .configure(routes)
            }
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        let auth = HttpTransport::new().server(&url).unwrap();
        let login = auth
            .signup("Alice".into(), "long password".into())
            .await
            .unwrap();
        let channel = login.client.channels().await.unwrap()[0].id.clone();
        for ix in 0..53 {
            let text = format!("line {ix}");
            assert_eq!(
                login
                    .client
                    .send_message(channel.clone(), text.clone())
                    .await
                    .unwrap()
                    .text,
                text
            );
        }
        db.db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "UPDATE messages SET created_at = '2026-01-01T00:00:00.000000000Z'",
            ))
            .await
            .unwrap();
        let activity = ConversationHandle::new(
            1,
            login.expires_at,
            login.client.clone(),
            Execution::controlled(cx.background_executor.clone(), login.expires_at - 3600),
        );
        activity.start(false);
        await_conversation(cx, &activity, |activity| {
            matches!(
                activity.read().history.get(&channel),
                Some(Load::Ready(messages)) if messages.len() == 50
            )
        })
        .await;
        assert_history(&activity, &channel, 50, 0);
        assert_eq!(activity.read().older.get(&channel), Some(&Older::Available));

        // A burst needs three real pages to reach the loaded segment. Only the
        // coordinator may follow the opaque server cursors and establish continuity.
        for ix in 53..158 {
            let text = format!("line {ix}");
            assert_eq!(
                login
                    .client
                    .send_message(channel.clone(), text.clone())
                    .await
                    .unwrap()
                    .text,
                text
            );
        }
        db.db
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "UPDATE messages SET created_at = '2027-01-01T00:00:00.000000000Z' WHERE created_at != '2026-01-01T00:00:00.000000000Z'",
            ))
            .await
            .unwrap();
        activity.refresh_history();
        await_conversation(cx, &activity, |activity| {
            !activity.read().refreshing.contains_key(&channel)
        })
        .await;
        // 105 new plus the 50 initially loaded; the oldest three remain paged.
        assert_history(&activity, &channel, 50, 105);
        assert_eq!(activity.read().older.get(&channel), Some(&Older::Available));
        activity.request_older();
        await_conversation(cx, &activity, |activity| {
            activity.read().older.get(&channel) == Some(&Older::Exhausted)
        })
        .await;
        assert_history(&activity, &channel, 53, 105);
        activity.close();
        handle.stop(true).await;
    });
}

fn assert_history(activity: &ConversationHandle, channel: &str, old: usize, new: usize) {
    let state = activity.read();
    let Some(Load::Ready(messages)) = state.history.get(channel) else {
        panic!("history missing");
    };
    assert_eq!(messages.len(), old + new);
    let lines = messages
        .iter()
        .map(|message| {
            message
                .text
                .strip_prefix("line ")
                .unwrap()
                .parse::<usize>()
                .unwrap()
        })
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        lines.len(),
        old + new,
        "every published message appears once"
    );
    assert_eq!(lines.iter().filter(|&&ix| ix < 53).count(), old);
    assert_eq!(
        lines.iter().filter(|&&ix| (53..158).contains(&ix)).count(),
        new
    );
    assert_eq!(
        messages
            .iter()
            .map(|message| &message.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        messages.len(),
        "overlapping catch-up pages must not duplicate identities"
    );
    assert!(
        messages
            .iter()
            .all(|message| message.created_at.starts_with(
                if message.text[5..].parse::<usize>().unwrap() < 53 {
                    "2026-01-01"
                } else {
                    "2027-01-01"
                }
            ))
    );
    assert!(
        messages
            .windows(2)
            .all(|pair| pair[0].created_at > pair[1].created_at
                || (pair[0].created_at == pair[1].created_at
                    && pair[0].id.parse::<i64>().unwrap() > pair[1].id.parse::<i64>().unwrap())),
        "server timestamp and ID tie order must survive decoding and catch-up"
    );
}
