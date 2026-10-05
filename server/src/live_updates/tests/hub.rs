use super::*;
use hamlet_protocol::{Channel, ChannelType};

async fn frame(subscription: &mut Subscription) -> Option<Bytes> {
    tokio::time::timeout(std::time::Duration::from_secs(2), subscription.next_frame())
        .await
        .expect("bounded hub frame")
}

pub(super) fn change() -> PreparedEvent {
    PreparedEvent::new(&Event::ChannelCreated {
        channel: Channel {
            id: "100000000000001".into(),
            name: "general".into(),
            kind: ChannelType::Text,
        },
    })
    .unwrap()
}

#[test]
fn unknown_events_cannot_be_prepared_for_publication() {
    assert!(PreparedEvent::new(&Event::Unknown).is_err());
}

#[tokio::test]
async fn no_subscribers_is_normal_and_fanout_shares_payload_storage() {
    let hub = EventHub::default();
    hub.notify(change());
    let mut first = hub.subscribe();
    let mut second = hub.clone().subscribe();
    assert_eq!(
        frame(&mut first).await.unwrap(),
        "event: ready\ndata: {}\n\n"
    );
    frame(&mut second).await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), first.next_frame())
            .await
            .is_err(),
        "no replay of pre-subscription changes"
    );
    hub.notify(change());
    let one = frame(&mut first).await.unwrap();
    let two = frame(&mut second).await.unwrap();
    assert_eq!(one, two);
    assert_eq!(
        one.as_ptr(),
        two.as_ptr(),
        "shared immutable payload allocation"
    );
    drop(first);
    hub.notify(change());
    assert!(
        frame(&mut second)
            .await
            .unwrap()
            .starts_with(b"event: change\n")
    );
}

#[tokio::test]
async fn lag_closes_only_the_affected_subscription_and_fresh_delivery_remains_available() {
    let hub = EventHub::default();
    let mut slow = hub.subscribe();
    let mut healthy = hub.subscribe();
    frame(&mut healthy).await.unwrap();
    for _ in 0..257 {
        hub.notify(change());
        assert!(
            frame(&mut healthy)
                .await
                .unwrap()
                .starts_with(b"event: change\n")
        );
    }
    assert!(frame(&mut slow).await.is_none());
    let mut fresh = hub.subscribe();
    assert_eq!(
        frame(&mut fresh).await.unwrap(),
        "event: ready\ndata: {}\n\n"
    );
    hub.notify(change());
    assert!(frame(&mut slow).await.is_none(), "lag is terminal");
    assert_eq!(frame(&mut healthy).await, frame(&mut fresh).await);
}

#[actix_web::test]
async fn terminal_http_bodies_release_lagged_subscription_without_waiting_for_client_drop() {
    use crate::{connect_to_database, routes};
    use actix_web::{App, body::MessageBody, test, web};
    use serde_json::{Value, json};
    use std::{pin::Pin, time::Duration};

    let state = connect_to_database("sqlite::memory:").await.unwrap();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(routes),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/auth/signup")
            .set_json(json!({"username":"Alice", "password":"long password"}))
            .to_request(),
    )
    .await;
    let auth: Value = test::read_body_json(response).await;
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/events")
            .insert_header((
                "Authorization",
                format!("Bearer {}", auth["access_token"].as_str().unwrap()),
            ))
            .to_request(),
    )
    .await;
    let mut body = response.into_body();
    assert_eq!(state.events.sender.receiver_count(), 1);
    for _ in 0..257 {
        state.events.notify(change());
    }
    let terminal = tokio::time::timeout(
        Duration::from_secs(2),
        std::future::poll_fn(|cx| Pin::new(&mut body).poll_next(cx)),
    )
    .await
    .unwrap();
    assert!(terminal.is_none());
    assert_eq!(state.events.sender.receiver_count(), 0);
    assert_eq!(state.events.sender.len(), 0);
    drop(body);
}

#[tokio::test]
async fn full_capacity_is_deliverable_but_one_more_before_ready_is_terminal() {
    let hub = EventHub::default();
    let mut subscriber = hub.subscribe();
    for _ in 0..256 {
        hub.notify(change());
    }
    assert_eq!(
        frame(&mut subscriber).await.unwrap(),
        "event: ready\ndata: {}\n\n"
    );
    for _ in 0..256 {
        assert!(
            frame(&mut subscriber)
                .await
                .unwrap()
                .starts_with(b"event: change\n")
        );
    }
    let mut late = hub.subscribe();
    for _ in 0..257 {
        hub.notify(change());
    }
    assert!(frame(&mut late).await.is_none());
    assert!(frame(&mut late).await.is_none());
}
