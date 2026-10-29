//! Client binding and Chat deletion observations against real isolated routes.
use super::*;

#[gpui_kit::test]
fn real_message_delete_filters_held_newest_older_and_send_responses_without_backfill(
    cx: &mut TestAppContext,
) {
    actix_web::rt::System::new().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let state = hamlet::connect_to_database(&format!("sqlite://{}?mode=rwc", dir.path().join("message-delete.db").display())).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = serve(listener, state);
        let handle = server.handle();
        let task = actix_web::rt::spawn(server);
        let reads = Arc::new(AtomicUsize::new(0));
        let history_gate = Gate::new();
        let post_gate = Gate::new();
        let api = HttpTransport::with_adapter(Arc::new(ControlledResponses {
            inner: CountReads { reads: reads.clone(), client: reqwest::Client::builder().no_proxy().build().unwrap() },
            channels: Gate::new(), history: history_gate.clone(), post: post_gate.clone(),
            fail_history: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            lose_post: Arc::new(std::sync::atomic::AtomicBool::new(false)), writes: Arc::new(AtomicUsize::new(0)),
        })).server(&url).unwrap();
        let alice = api.signup("Alice".into(), "long password".into()).await.unwrap();
        let plain = HttpTransport::new().server(&url).unwrap();
        // Another authenticated session for the same persistent author, not a Chat delete workflow.
        let author = plain.login("Alice".into(), "long password".into()).await.unwrap();
        let bob = plain.signup("Bob".into(), "long password".into()).await.unwrap();
        let general = author.client.channels().await.unwrap()[0].id.clone();
        let other = author.client.create_channel("other".into()).await.unwrap().id;
        let mut messages = Vec::new();
        for n in 0..55 {
            messages.push(author.client.send_message(other.clone(), format!("history {n}")).await.unwrap());
        }
        let chat = ChatHandle::new(7, alice.user.id.clone(), alice.expires_at, alice.client, Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        chat.start();
        settle(cx, &[&chat], || chat.status().is_empty() && matches!(chat.read().history(&general), Some(Load::Ready(_)))).await;
        assert_eq!(bob.client.delete_message(other.clone(), messages[54].id.clone()).await, Err(ApiError::Forbidden));
        assert_eq!(author.client.delete_message(general.clone(), messages[54].id.clone()).await, Err(ApiError::NotFound));
        assert_eq!(author.client.delete_message("invalid".into(), messages[54].id.clone()).await, Err(ApiError::InvalidInput));
        history_gate.arm();
        chat.select_channel(&other);
        settle(cx, &[&chat], || history_gate.reached.load(Ordering::SeqCst)).await;
        let baseline = reads.load(Ordering::SeqCst);
        author.client.delete_message(other.clone(), messages[54].id.clone()).await.unwrap();
        // Barrier: a later creation is processed after deletion on the same ordered stream.
        let barrier = author.client.send_message(general.clone(), "newest barrier".into()).await.unwrap();
        settle(cx, &[&chat], || ids(&chat, &general).contains(&barrier.id)).await;
        assert_eq!(reads.load(Ordering::SeqCst), baseline);
        history_gate.open();
        settle(cx, &[&chat], || matches!(chat.read().history(&other), Some(Load::Ready(_)))).await;
        assert_eq!(ids(&chat, &other).len(), 49);
        assert!(!ids(&chat, &other).contains(&messages[54].id));
        assert_eq!(chat.read().older(&other), Some(&OlderPages::Available));
        history_gate.arm(); chat.request_older();
        settle(cx, &[&chat], || history_gate.reached.load(Ordering::SeqCst)).await;
        author.client.delete_message(other.clone(), messages[0].id.clone()).await.unwrap();
        let barrier = author.client.send_message(general.clone(), "older barrier".into()).await.unwrap();
        settle(cx, &[&chat], || ids(&chat, &general).contains(&barrier.id)).await;
        history_gate.open();
        settle(cx, &[&chat], || chat.read().older(&other) == Some(&OlderPages::Exhausted)).await;
        assert_eq!(ids(&chat, &other).len(), 53);
        assert!(!ids(&chat, &other).contains(&messages[0].id));
        assert_eq!(reads.load(Ordering::SeqCst), baseline + 1, "only the deliberate older read");
        chat.select_channel(&general); chat.edit_draft("held send".into());
        post_gate.arm(); chat.send();
        settle(cx, &[&chat], || post_gate.reached.load(Ordering::SeqCst) && matches!(chat.read().history(&general), Some(Load::Ready(rows)) if rows.iter().any(|m| m.text == "held send"))).await;
        let sent_id = match chat.read().history(&general) {
            Some(Load::Ready(rows)) => rows.iter().find(|m| m.text == "held send").unwrap().id.clone(),
            _ => unreachable!(),
        };
        author.client.delete_message(general.clone(), sent_id.clone()).await.unwrap();
        settle(cx, &[&chat], || !ids(&chat, &general).contains(&sent_id)).await;
        assert!(chat.read().is_sending(&general));
        post_gate.open();
        settle(cx, &[&chat], || !chat.read().is_sending(&general)).await;
        assert!(!ids(&chat, &general).contains(&sent_id));
        assert_eq!(chat.read().draft(&general), "");
        assert_eq!(reads.load(Ordering::SeqCst), baseline + 1);
        author.client.delete_message(general.clone(), sent_id.clone()).await.unwrap();
        assert_eq!(bob.client.delete_message(general.clone(), sent_id.clone()).await, Err(ApiError::Forbidden));
        assert!(!author.client.history_page(general, None).await.unwrap().items.iter().any(|m| m.id == sent_id));
        chat.close(); stop(handle, task).await;
    });
}
