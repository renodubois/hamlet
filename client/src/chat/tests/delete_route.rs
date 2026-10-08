use super::*;

#[gpui_kit::test]
fn remote_delete_empty_fallback_and_missed_deletion_survives_reconnect_without_reads(
    cx: &mut TestAppContext,
) {
    actix_web::rt::System::new().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let db = hamlet::connect_to_database(&format!("sqlite://{}?mode=rwc", dir.path().join("missed-delete.db").display())).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = serve(listener, db); let handle = server.handle(); let task = actix_web::rt::spawn(server);
        let reads = Arc::new(AtomicUsize::new(0));
        let relay = RelayTasks::default(); let gate = Gate::new();
        let api = HttpTransport::with_adapters(Arc::new(CountReads { reads: reads.clone(), client: reqwest::Client::builder().no_proxy().build().unwrap() }), Arc::new(relay.stream(gate, Arc::new(AtomicUsize::new(0))))).server(&url).unwrap();
        let alice = api.signup("Alice".into(), "long password".into()).await.unwrap();
        let bob = HttpTransport::new().server(&url).unwrap().signup("Bob".into(), "long password".into()).await.unwrap();
        let general = alice.client.channels().await.unwrap()[0].clone();
        let a = ChatHandle::new(1, alice.expires_at, alice.client.clone(), Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        a.start();
        settle(cx, &[&a], || a.status().is_empty() && matches!(a.read().history.get(&general.id), Some(Load::Ready(_)))).await;
        a.edit_draft("discard remotely".into());
        let baseline = reads.load(Ordering::SeqCst);
        relay.disconnect(); settle(cx, &[&a], || !a.status().is_empty()).await;
        bob.client.create_channel("missed replacement".into()).await.unwrap();
        cx.background_executor.advance_clock(Duration::from_secs(3));
        settle(cx, &[&a], || a.status().is_empty()).await;
        assert_eq!(reads.load(Ordering::SeqCst), baseline);
        bob.client.delete_channel(general.id.clone()).await.unwrap();
        settle(cx, &[&a], || a.read().selected.is_none()).await;
        assert!(a.read().history.is_empty()); assert!(a.read().drafts.is_empty());
        assert!(matches!(&a.read().channels, Some(Load::Ready(channels)) if channels.is_empty()));
        assert_eq!(reads.load(Ordering::SeqCst), baseline, "remote empty fallback performs no reads");
        let later = bob.client.create_channel("later".into()).await.unwrap();
        settle(cx, &[&a], || matches!(&a.read().channels, Some(Load::Ready(channels)) if channels.iter().any(|c| c.id == later.id))).await;
        a.select_channel(&later.id); settle(cx, &[&a], || matches!(a.read().history.get(&later.id), Some(Load::Ready(_)))).await;
        a.edit_draft("accepted stale draft".into());
        let baseline = reads.load(Ordering::SeqCst);
        relay.disconnect(); settle(cx, &[&a], || !a.status().is_empty()).await;
        bob.client.delete_channel(later.id.clone()).await.unwrap();
        cx.background_executor.advance_clock(Duration::from_secs(3)); settle(cx, &[&a], || a.status().is_empty()).await;
        assert_eq!(a.read().selected.as_deref(), Some(later.id.as_str()));
        assert_eq!(a.read().draft(&later.id), "accepted stale draft");
        assert!(matches!(a.read().history.get(&later.id), Some(Load::Ready(_))));
        assert_eq!(reads.load(Ordering::SeqCst), baseline, "no catch-up after missed deletion");
        assert_eq!(bob.client.history_page(later.id, None).await, Err(ApiError::NotFound));
        a.close();
        let reopened = ChatHandle::new(2, alice.expires_at, alice.client, Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        reopened.start(); settle(cx, &[&reopened], || matches!(&reopened.read().channels, Some(Load::Ready(channels)) if channels.len() == 1 && channels[0].name == "missed replacement")).await;
        reopened.close(); drop(relay); stop(handle, task).await;
    });
}

#[gpui_kit::test]
fn two_user_live_delete_both_orders_cleanup_fallback_and_reopening(cx: &mut TestAppContext) {
    actix_web::rt::System::new().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let db = hamlet::connect_to_database(&format!("sqlite://{}?mode=rwc", dir.path().join("shared-delete.db").display())).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = serve(listener, db);
        let handle = server.handle();
        let task = actix_web::rt::spawn(server);
        let reads = Arc::new(AtomicUsize::new(0));
        let writes = Arc::new(AtomicUsize::new(0));
        let response_gate = Gate::new();
        let stream_gate = Gate::new();
        let relays = RelayTasks::default();
        let api = HttpTransport::with_adapters(Arc::new(ControlledResponses {
            inner: CountReads { reads: reads.clone(), client: reqwest::Client::builder().no_proxy().build().unwrap() },
            channels: Gate::new(), history: Gate::new(), post: response_gate.clone(),
            fail_history: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            lose_post: Arc::new(std::sync::atomic::AtomicBool::new(false)), writes: writes.clone(),
        }), Arc::new(relays.stream(stream_gate.clone(), Arc::new(AtomicUsize::new(0))))).server(&url).unwrap();
        let alice = api.signup("Alice".into(), "long password".into()).await.unwrap();
        let bob = api.signup("Bob".into(), "long password".into()).await.unwrap();
        let general = alice.client.channels().await.unwrap()[0].clone();
        let first = bob.client.create_channel("aaa-first".into()).await.unwrap();
        let other = bob.client.create_channel("zzz-other".into()).await.unwrap();
        let a = ChatHandle::new(1, alice.expires_at, alice.client.clone(), Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        let b = ChatHandle::new(2, bob.expires_at, bob.client.clone(), Execution::controlled(cx.background_executor.clone(), bob.expires_at - 3600));
        a.start(); b.start();
        settle(cx, &[&a, &b], || a.status().is_empty() && b.status().is_empty() && matches!(a.read().history.get(&first.id), Some(Load::Ready(_))) && matches!(b.read().history.get(&first.id), Some(Load::Ready(_)))).await;
        a.edit_draft("discard Alice".into()); b.edit_draft("discard Bob".into());
        // Alice removes an unselected identity; Bob selected it and has a draft.
        b.select_channel(&other.id);
        settle(cx, &[&a, &b], || matches!(b.read().history.get(&other.id), Some(Load::Ready(_)))).await;
        b.edit_draft("other draft".into());
        let baseline = reads.load(Ordering::SeqCst);
        response_gate.arm();
        a.delete_channel(&other.id);
        settle(cx, &[&a, &b], || response_gate.reached.load(Ordering::SeqCst) && !b.read().history.contains_key(&other.id) && matches!(&a.read().channels, Some(Load::Ready(channels)) if !channels.iter().any(|c| c.id == other.id))).await;
        assert!(a.read().delete_pending);
        assert_eq!(a.read().delete_confirmed, 0);
        assert_eq!(a.read().selected.as_deref(), Some(first.id.as_str()));
        assert_eq!(b.read().selected.as_deref(), Some(first.id.as_str()));
        assert!(!b.read().drafts.contains_key(&other.id));
        assert_eq!(reads.load(Ordering::SeqCst), baseline, "cached fallback needs no history read");
        response_gate.open();
        settle(cx, &[&a, &b], || !a.read().delete_pending).await;
        assert_eq!(a.read().delete_confirmed, 1);
        // HTTP first: neither client has loaded the first remaining channel's history.
        stream_gate.arm();
        b.delete_channel(&first.id);
        settle(cx, &[&a, &b], || !b.read().delete_pending && stream_gate.reached.load(Ordering::SeqCst) && matches!(b.read().history.get(&general.id), Some(Load::Ready(_)))).await;
        assert_eq!(a.read().selected.as_deref(), Some(first.id.as_str()));
        stream_gate.open(); stream_gate.open();
        settle(cx, &[&a, &b], || matches!(a.read().history.get(&general.id), Some(Load::Ready(_)))).await;
        for chat in [&a, &b] {
            assert_eq!(chat.read().selected.as_deref(), Some(general.id.as_str()));
            assert!(!chat.read().history.contains_key(&first.id));
            assert!(!chat.read().drafts.contains_key(&first.id));
        }
        assert_eq!(reads.load(Ordering::SeqCst), baseline + 2, "one ordinary fallback read per client, no reconciliation");
        assert_eq!(writes.load(Ordering::SeqCst), 2);
        // Last-channel failure leaves both chats intact.
        a.delete_channel(&general.id);
        settle(cx, &[&a, &b], || !a.read().delete_pending).await;
        assert!(a.read().delete_feedback.as_ref().unwrap().contains("last active"));
        assert_eq!(b.read().selected.as_deref(), Some(general.id.as_str()));
        a.close(); b.close();
        let reopened = ChatHandle::new(3, alice.expires_at, alice.client, Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        reopened.start();
        settle(cx, &[&reopened], || matches!(&reopened.read().channels, Some(Load::Ready(channels)) if channels.len() == 1)).await;
        assert_eq!(reopened.read().selected.as_deref(), Some(general.id.as_str()));
        reopened.close(); drop(relays); stop(handle, task).await;
    });
}

#[gpui_kit::test]
fn real_route_delete_rejects_late_send_and_live_delivery_with_no_locally_known_fallback(
    cx: &mut TestAppContext,
) {
    actix_web::rt::System::new().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let db = hamlet::connect_to_database(&format!("sqlite://{}?mode=rwc", dir.path().join("late-delete.db").display())).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = serve(listener, db);
        let handle = server.handle();
        let task = actix_web::rt::spawn(server);
        let reads = Arc::new(AtomicUsize::new(0));
        let post_gate = Gate::new();
        let stream_gate = Gate::new();
        let relays = RelayTasks::default();
        let api = HttpTransport::with_adapters(Arc::new(HeldPost {
            inner: CountReads { reads: reads.clone(), client: reqwest::Client::builder().no_proxy().build().unwrap() },
            gate: post_gate.clone(),
        }), Arc::new(relays.stream(stream_gate.clone(), Arc::new(AtomicUsize::new(0))))).server(&url).unwrap();
        let alice = api.signup("Alice".into(), "long password".into()).await.unwrap();
        let bob = HttpTransport::new().server(&url).unwrap().signup("Bob".into(), "long password".into()).await.unwrap();
        let general = alice.client.channels().await.unwrap()[0].clone();
        let a = ChatHandle::new(1, alice.expires_at, alice.client, Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        a.start();
        settle(cx, &[&a], || a.status().is_empty() && matches!(a.read().history.get(&general.id), Some(Load::Ready(_)))).await;
        // Server knows another active channel, but its creation is held in the stream.
        stream_gate.arm();
        bob.client.create_channel("other".into()).await.unwrap();
        settle(cx, &[&a], || stream_gate.reached.load(Ordering::SeqCst)).await;
        let baseline = reads.load(Ordering::SeqCst);
        a.edit_draft("retained until delete".into());
        post_gate.arm();
        a.send();
        settle(cx, &[&a], || post_gate.reached.load(Ordering::SeqCst)).await;
        assert!(a.read().send_pending.contains_key(&general.id));
        a.delete_channel(&general.id);
        settle(cx, &[&a], || !a.read().delete_pending).await;
        assert_eq!(a.read().delete_confirmed, 1);
        assert!(a.read().selected.is_none());
        assert!(a.read().history.is_empty());
        assert!(a.read().drafts.is_empty());
        assert!(a.read().send_pending.is_empty());
        assert_eq!(reads.load(Ordering::SeqCst), baseline, "empty fallback causes no invented history or reconciliation read");
        post_gate.open(); stream_gate.open();
        settle(cx, &[&a], || matches!(&a.read().channels, Some(Load::Ready(channels)) if channels.len() == 1)).await;
        // Drain future stream bytes through the same barrier: the old message must not allocate history.
        let later = bob.client.create_channel("later".into()).await.unwrap();
        settle(cx, &[&a], || matches!(&a.read().channels, Some(Load::Ready(channels)) if channels.iter().any(|c| c.id == later.id))).await;
        assert!(a.read().selected.is_none(), "late creation never changes empty selection");
        assert!(a.read().history.is_empty());
        assert!(a.read().drafts.is_empty());
        assert!(a.read().send_feedback.is_empty());
        assert_eq!(reads.load(Ordering::SeqCst), baseline);
        a.close(); drop(relays); stop(handle, task).await;
    });
}

#[gpui_kit::test]
fn real_route_delete_target_cleanup_fallback_uncertainty_and_session_isolation(
    cx: &mut TestAppContext,
) {
    actix_web::rt::System::new().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let db = hamlet::connect_to_database(&format!("sqlite://{}?mode=rwc", dir.path().join("delete.db").display())).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = serve(listener, db);
        let handle = server.handle();
        let task = actix_web::rt::spawn(server);
        let reads = Arc::new(AtomicUsize::new(0));
        let writes = Arc::new(AtomicUsize::new(0));
        let gate = Gate::new();
        let history_gate = Gate::new();
        let lose = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let api = HttpTransport::with_adapter(Arc::new(ControlledResponses {
            inner: CountReads { reads: reads.clone(), client: reqwest::Client::builder().no_proxy().build().unwrap() },
            channels: Gate::new(), history: history_gate.clone(), post: gate.clone(),
            fail_history: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            lose_post: lose.clone(), writes: writes.clone(),
        })).server(&url).unwrap();
        let alice = api.signup("Alice".into(), "long password".into()).await.unwrap();
        let bob = HttpTransport::new().server(&url).unwrap().signup("Bob".into(), "long password".into()).await.unwrap();
        let general = alice.client.channels().await.unwrap()[0].clone();
        let other = bob.client.create_channel("zz-other".into()).await.unwrap();
        let a = ChatHandle::new(1, alice.expires_at, alice.client.clone(), Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        a.start();
        settle(cx, &[&a], || matches!(a.read().history.get(&general.id), Some(Load::Ready(_)))).await;
        a.edit_draft("general draft".into());
        a.select_channel(&other.id);
        settle(cx, &[&a], || matches!(a.read().history.get(&other.id), Some(Load::Ready(_)))).await;
        a.edit_draft("other draft".into());
        a.select_channel(&general.id);
        let baseline = reads.load(Ordering::SeqCst);
        gate.arm();
        a.delete_channel(&other.id);
        a.delete_channel(&general.id);
        settle(cx, &[&a], || gate.reached.load(Ordering::SeqCst)).await;
        assert!(a.read().delete_pending);
        // A live deletion may already clean state, but cannot confirm the dialog.
        assert_eq!(a.read().delete_confirmed, 0);
        gate.open();
        settle(cx, &[&a], || !a.read().delete_pending).await;
        assert_eq!(writes.load(Ordering::SeqCst), 1, "duplicate intention sends no request");
        assert_eq!(a.read().selected.as_deref(), Some(general.id.as_str()));
        assert_eq!(a.read().draft(&general.id), "general draft");
        assert!(!a.read().history.contains_key(&other.id));
        assert!(!a.read().drafts.contains_key(&other.id));
        a.edit_channel_draft(&other.id, "late edit".into());
        assert!(!a.read().drafts.contains_key(&other.id));
        assert_eq!(reads.load(Ordering::SeqCst), baseline);
        a.delete_channel(&general.id);
        settle(cx, &[&a], || !a.read().delete_pending).await;
        assert!(a.read().delete_feedback.as_ref().unwrap().contains("last active channel"));
        assert_eq!(a.read().draft(&general.id), "general draft");
        // Replacement name has a new identity and empty ordinary history.
        let replacement = bob.client.create_channel("ZZ-OTHER".into()).await.unwrap();
        assert_ne!(replacement.id, other.id);
        settle(cx, &[&a], || matches!(&a.read().channels, Some(Load::Ready(channels)) if channels.iter().any(|c| c.id == replacement.id))).await;
        let baseline = reads.load(Ordering::SeqCst);
        a.delete_channel(&general.id);
        settle(cx, &[&a], || !a.read().delete_pending && matches!(a.read().history.get(&replacement.id), Some(Load::Ready(_)))).await;
        assert_eq!(a.read().selected.as_deref(), Some(replacement.id.as_str()));
        assert_eq!(reads.load(Ordering::SeqCst), baseline + 1, "only ordinary fallback history read");
        assert!(!a.read().drafts.contains_key(&general.id));
        // A peer removes the channel: stale local deletion receives actionable 404.
        let hidden = bob.client.create_channel("zz-hidden".into()).await.unwrap();
        settle(cx, &[&a], || matches!(&a.read().channels, Some(Load::Ready(channels)) if channels.iter().any(|c| c.id == hidden.id))).await;
        bob.client.delete_channel(hidden.id.clone()).await.unwrap();
        a.delete_channel(&hidden.id);
        settle(cx, &[&a], || !a.read().delete_pending).await;
        assert!(a.read().delete_feedback.as_ref().unwrap().contains("not found"));
        let uncertain = bob.client.create_channel("zz-uncertain".into()).await.unwrap();
        settle(cx, &[&a], || matches!(&a.read().channels, Some(Load::Ready(channels)) if channels.iter().any(|c| c.id == uncertain.id))).await;
        let baseline = reads.load(Ordering::SeqCst);
        let before = writes.load(Ordering::SeqCst);
        lose.store(true, Ordering::SeqCst);
        a.delete_channel(&uncertain.id);
        settle(cx, &[&a], || !a.read().delete_pending).await;
        assert!(a.read().delete_feedback.as_ref().unwrap().contains("may have succeeded"));
        assert_eq!(writes.load(Ordering::SeqCst), before + 1);
        assert_eq!(reads.load(Ordering::SeqCst), baseline);
        settle(cx, &[&a], || matches!(&a.read().channels, Some(Load::Ready(channels)) if !channels.iter().any(|c| c.id == uncertain.id))).await;
        assert!(a.read().delete_feedback.as_ref().unwrap().contains("may have succeeded"), "event does not turn uncertainty into success");
        // Hold a real history snapshot until after local deletion confirmation.
        let late = bob.client.create_channel("zz-late".into()).await.unwrap();
        settle(cx, &[&a], || matches!(&a.read().channels, Some(Load::Ready(channels)) if channels.iter().any(|c| c.id == late.id))).await;
        history_gate.arm();
        a.select_channel(&late.id);
        settle(cx, &[&a], || history_gate.reached.load(Ordering::SeqCst)).await;
        a.delete_channel(&late.id);
        settle(cx, &[&a], || !a.read().delete_pending).await;
        history_gate.open();
        cx.executor().run_until_parked();
        assert!(!a.read().history.contains_key(&late.id));
        let last = bob.client.create_channel("zz-session".into()).await.unwrap();
        settle(cx, &[&a], || matches!(&a.read().channels, Some(Load::Ready(channels)) if channels.iter().any(|c| c.id == last.id))).await;
        gate.arm();
        a.delete_channel(&last.id);
        settle(cx, &[&a], || gate.reached.load(Ordering::SeqCst)).await;
        a.close(); gate.open();
        cx.executor().run_until_parked();
        assert!(a.read().channels.is_none());
        let b = ChatHandle::new(2, bob.expires_at, bob.client, Execution::controlled(cx.background_executor.clone(), bob.expires_at - 3600));
        b.start();
        settle(cx, &[&b], || matches!(b.read().channels, Some(Load::Ready(_)))).await;
        assert_eq!(b.read().delete_confirmed, 0);
        assert!(!b.read().delete_pending);
        b.close(); stop(handle, task).await;
    });
}
