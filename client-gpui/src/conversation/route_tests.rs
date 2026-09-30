use super::state::{Conversation, Identity, Load, ReadRequest};
use crate::api::HttpTransport;
use std::{net::TcpListener, time::Duration};

#[actix_web::test]
async fn second_user_activity_is_found_by_focused_polling_against_unchanged_routes() {
    use crate::conversation::polling::{Polling, Resource};
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
    let mut identity = Identity {
        generation: Some(1),
        expires_at: alice.expires_at,
        rejected: None,
    };
    let mut conversation = Conversation::default();
    let list = conversation.start(&identity).unwrap();
    let channels = alice.client.channels().await.unwrap();
    let initial = conversation
        .complete_channels(&mut identity, &list, Ok(channels))
        .unwrap();
    let page = alice
        .client
        .history_page(channel.clone(), None)
        .await
        .unwrap();
    conversation.complete_history(&mut identity, &initial, Ok(page));
    let mut poll = Polling::default();
    assert!(poll.focus(true, Duration::ZERO));
    poll.started(Resource::History);
    poll.started(Resource::Channels);
    poll.completed(Resource::History, true, Duration::ZERO);
    poll.completed(Resource::Channels, true, Duration::ZERO);
    let posted = bob
        .client
        .send_message(
            channel.clone(),
            "Bob published while Alice was reading".into(),
        )
        .await
        .unwrap();
    let new_channel = bob.client.create_channel("Bob room".into()).await.unwrap();
    assert!(poll.due(Resource::History, Duration::from_secs(3)));
    let refresh = conversation.refresh_history(&identity).unwrap();
    let mut request = refresh;
    loop {
        let page = alice
            .client
            .history_page(channel.clone(), request.before.clone())
            .await
            .unwrap();
        let outcome = conversation.complete_history(&mut identity, &request, Ok(page));
        if let Some(next) = outcome.next {
            request = next;
        } else {
            break;
        }
    }
    assert!(
        matches!(conversation.history.get(&channel), Some(Load::Ready(messages)) if messages.iter().any(|m| m.id == posted.id && m.author_name == "Bob"))
    );
    assert!(poll.due(Resource::Channels, Duration::from_secs(15)));
    let listing = conversation.refresh_channels(&identity).unwrap();
    let channels = alice.client.channels().await.unwrap();
    conversation.complete_channels(&mut identity, &listing, Ok(channels));
    assert!(
        matches!(conversation.channels, Some(Load::Ready(ref channels)) if channels.contains(&new_channel))
    );
    handle.stop(true).await;
}

#[actix_web::test]
async fn rewrite_history_traverses_multiple_pages_with_timestamp_ties() {
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
    let first = login
        .client
        .history_page(channel.clone(), None)
        .await
        .unwrap();
    assert_eq!(first.items.len(), 50);
    let cursor = first
        .next_cursor
        .clone()
        .expect("a server-issued next cursor");
    let last = login
        .client
        .history_page(channel.clone(), Some(cursor))
        .await
        .unwrap();
    assert_eq!(last.items.len(), 3);
    assert_eq!(last.next_cursor, None);
    let ids: Vec<_> = first
        .items
        .iter()
        .chain(&last.items)
        .map(|m| m.id.parse::<i64>().unwrap())
        .collect();
    assert!(
        ids.windows(2).all(|pair| pair[0] > pair[1]),
        "server tie order must survive decoding"
    );
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        53
    );
    assert!(
        first
            .items
            .iter()
            .chain(&last.items)
            .all(|m| m.created_at.starts_with("2026-01-01"))
    );
    // Catch up through the real route, not just adapter pagination: a burst spans
    // multiple pages before it can reach the previously loaded 53-message segment.
    let mut identity = Identity {
        generation: Some(1),
        expires_at: login.expires_at,
        rejected: None,
    };
    let mut conversation = Conversation::default();
    let channels = conversation.start(&identity).unwrap();
    let list = login.client.channels().await.unwrap();
    let initial = conversation
        .complete_channels(&mut identity, &channels, Ok(list))
        .unwrap();
    let read = |request: &ReadRequest| {
        login
            .client
            .history_page(request.channel_id.clone().unwrap(), request.before.clone())
    };
    let first_page = read(&initial).await.unwrap();
    conversation.complete_history(&mut identity, &initial, Ok(first_page));
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
    let mut request = conversation.refresh_history(&identity).unwrap();
    let mut traversed = 0;
    loop {
        let page = read(&request).await.unwrap();
        let outcome = conversation.complete_history(&mut identity, &request, Ok(page));
        traversed += 1;
        if let Some(next) = outcome.next {
            request = next;
        } else {
            break;
        }
    }
    assert!(
        traversed >= 3,
        "catch-up must traverse beyond two new pages"
    );
    let Load::Ready(messages) = conversation.history.get(&channel).unwrap() else {
        panic!("history missing");
    };
    assert_eq!(messages.len(), 155); // 105 new plus the 50 initially loaded; older 3 remain paged
    assert!(
        messages
            .windows(2)
            .all(|pair| pair[0].created_at > pair[1].created_at
                || (pair[0].created_at == pair[1].created_at
                    && pair[0].id.parse::<i64>().unwrap() > pair[1].id.parse::<i64>().unwrap())),
        "server timestamp and ID tie order must survive catch-up"
    );
    assert!(!conversation.refreshing.contains_key(&channel));
    handle.stop(true).await;
}
