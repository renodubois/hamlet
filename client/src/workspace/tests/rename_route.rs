use super::*;

#[gpui_kit::test]
fn real_route_rename_preserves_conversation_and_rejects_closed_session(cx: &mut TestAppContext) {
    actix_web::rt::System::new().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let db = hamlet::connect_to_database(&format!("sqlite://{}?mode=rwc", dir.path().join("rename.db").display())).await.unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = serve(listener, db);
        let handle = server.handle();
        let task = actix_web::rt::spawn(server);
        let reads = Arc::new(AtomicUsize::new(0));
        let gate = Gate::new();
        let lose = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writes = Arc::new(AtomicUsize::new(0));
        let api = HttpTransport::with_adapter(Arc::new(ControlledResponses {
            inner: CountReads { reads: reads.clone(), client: reqwest::Client::builder().no_proxy().build().unwrap() },
            channels: Gate::new(), history: Gate::new(), post: gate.clone(),
            fail_history: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            lose_post: lose.clone(), writes: writes.clone(),
        })).server(&url).unwrap();
        let alice = api.signup("Alice".into(), "long password".into()).await.unwrap();
        let bob = api.signup("Bob".into(), "long password".into()).await.unwrap();
        let channel = alice.client.channels().await.unwrap()[0].clone();
        let other = bob.client.create_channel("Other".into()).await.unwrap();
        let message = bob.client.send_message(channel.id.clone(), "retained message".into()).await.unwrap();
        let activity = WorkspaceHandle::new(1, alice.expires_at, alice.client.clone(), Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        activity.start();
        settle(cx, &[&activity], || activity.status().is_empty() && ids(&activity, &channel.id).contains(&message.id)).await;
        activity.edit_draft("retained draft".into());
        let baseline = reads.load(Ordering::SeqCst);
        let first_writes = writes.load(Ordering::SeqCst);
        gate.arm();
        activity.rename_channel(&other.id, "  AAA  ");
        activity.rename_channel(&channel.id, "duplicate must not run");
        settle(cx, &[&activity], || gate.reached.load(Ordering::SeqCst)).await;
        assert!(activity.read().rename_pending);
        assert_eq!(activity.read().selected.as_deref(), Some(channel.id.as_str()));
        gate.open();
        settle(cx, &[&activity], || !activity.read().rename_pending).await;
        assert!(activity.read().rename_feedback.is_none());
        assert_eq!(writes.load(Ordering::SeqCst), first_writes + 1, "pending duplicate intention is ignored");
        assert!(matches!(&activity.read().channels, Some(Load::Ready(channels)) if channels[0].id == other.id && channels[0].name == "AAA"));
        for name in ["GENERAL", "GENERAL"] {
            activity.rename_channel(&channel.id, name);
            settle(cx, &[&activity], || !activity.read().rename_pending).await;
            assert!(activity.read().rename_feedback.is_none());
        }
        activity.rename_channel(&channel.id, "aaa");
        settle(cx, &[&activity], || !activity.read().rename_pending).await;
        assert!(activity.read().rename_feedback.as_ref().unwrap().contains("already exists"));
        assert_eq!(activity.read().selected.as_deref(), Some(channel.id.as_str()));
        assert_eq!(activity.read().draft(&channel.id), "retained draft");
        assert_eq!(ids(&activity, &channel.id), vec![message.id]);
        assert_eq!(reads.load(Ordering::SeqCst), baseline, "renames issue no reconciliation/history reads");
        let baseline_writes = writes.load(Ordering::SeqCst);
        lose.store(true, Ordering::SeqCst);
        activity.rename_channel(&channel.id, "accepted but unconfirmed");
        settle(cx, &[&activity], || !activity.read().rename_pending).await;
        assert!(activity.read().rename_feedback.as_ref().unwrap().contains("may have succeeded"));
        settle(cx, &[&activity], || activity.read().selected_channel().unwrap().name == "accepted but unconfirmed").await;
        assert_eq!(activity.read().draft(&channel.id), "retained draft");
        assert_eq!(reads.load(Ordering::SeqCst), baseline, "uncertainty issues no reads");
        assert_eq!(writes.load(Ordering::SeqCst), baseline_writes + 1, "uncertainty never replays a write");
        assert_eq!(bob.client.channels().await.unwrap().iter().find(|item| item.id == channel.id).unwrap().name, "accepted but unconfirmed");
        // Any authenticated user, not only a creator, may rename bootstrap.
        assert_eq!(bob.client.rename_channel(channel.id.clone(), "Bob name".into()).await.unwrap().id, channel.id);
        gate.arm();
        activity.rename_channel(&channel.id, "late completion");
        settle(cx, &[&activity], || gate.reached.load(Ordering::SeqCst)).await;
        activity.close();
        gate.open();
        cx.executor().run_until_parked();
        assert!(activity.read().channels.is_none());
        assert!(activity.read().drafts.is_empty());
        let replacement = WorkspaceHandle::new(2, bob.expires_at, bob.client.clone(), Execution::controlled(cx.background_executor.clone(), bob.expires_at - 3600));
        replacement.start();
        settle(cx, &[&replacement], || matches!(replacement.read().channels, Some(Load::Ready(_)))).await;
        assert!(!replacement.read().rename_pending);
        assert!(replacement.read().rename_feedback.is_none());
        replacement.close();
        stop(handle, task).await;
    });
}

#[gpui_kit::test]
fn two_user_live_rename_both_confirmation_orders_without_reads(cx: &mut TestAppContext) {
    actix_web::rt::System::new().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let db = hamlet::connect_to_database(&format!("sqlite://{}?mode=rwc", dir.path().join("shared.db").display())).await.unwrap();
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
        let channel = alice.client.channels().await.unwrap()[0].clone();
        let other = bob.client.create_channel("Other".into()).await.unwrap();
        let message = bob.client.send_message(channel.id.clone(), "retained".into()).await.unwrap();
        let a = WorkspaceHandle::new(1, alice.expires_at, alice.client, Execution::controlled(cx.background_executor.clone(), alice.expires_at - 3600));
        let b = WorkspaceHandle::new(2, bob.expires_at, bob.client, Execution::controlled(cx.background_executor.clone(), bob.expires_at - 3600));
        a.start(); b.start();
        settle(cx, &[&a, &b], || a.status().is_empty() && b.status().is_empty() && ids(&a, &channel.id).contains(&message.id) && ids(&b, &channel.id).contains(&message.id)).await;
        a.edit_draft("Alice draft".into()); b.edit_draft("Bob draft".into());
        let baseline = reads.load(Ordering::SeqCst);
        let named = |workspace: &WorkspaceHandle, id: &str, name: &str| matches!(&workspace.read().channels, Some(Load::Ready(channels)) if channels.iter().any(|c| c.id == id && c.name == name));
        // The live event arrives first: it cannot confirm the originating operation.
        response_gate.arm();
        a.rename_channel(&other.id, "AAA");
        settle(cx, &[&a, &b], || response_gate.reached.load(Ordering::SeqCst) && named(&a, &other.id, "AAA") && named(&b, &other.id, "AAA")).await;
        assert!(a.read().rename_pending);
        assert_eq!(a.read().rename_confirmed, 0);
        response_gate.open();
        settle(cx, &[&a, &b], || !a.read().rename_pending).await;
        // Both streams are gated: HTTP confirmation arrives first.
        stream_gate.arm();
        b.rename_channel(&channel.id, "ZZZ");
        settle(cx, &[&a, &b], || !b.read().rename_pending && named(&b, &channel.id, "ZZZ") && stream_gate.reached.load(Ordering::SeqCst)).await;
        assert!(named(&a, &channel.id, "general"));
        stream_gate.open(); stream_gate.open();
        settle(cx, &[&a, &b], || named(&a, &channel.id, "ZZZ")).await;
        // A failed mutation neither changes shared state nor produces extra reads.
        a.rename_channel(&channel.id, "aaa");
        settle(cx, &[&a, &b], || !a.read().rename_pending).await;
        assert!(a.read().rename_feedback.as_ref().unwrap().contains("already exists"));
        for workspace in [&a, &b] {
            assert_eq!(workspace.read().selected.as_deref(), Some(channel.id.as_str()));
            assert_eq!(ids(workspace, &channel.id), vec![message.id.clone()]);
            assert!(matches!(&workspace.read().channels, Some(Load::Ready(channels)) if channels.len() == 2 && channels[0].id == other.id && channels[1].id == channel.id));
        }
        assert_eq!(a.read().draft(&channel.id), "Alice draft");
        assert_eq!(b.read().draft(&channel.id), "Bob draft");
        assert_eq!(reads.load(Ordering::SeqCst), baseline);
        assert_eq!(writes.load(Ordering::SeqCst), 4); // seeded message plus three renames
        a.close(); b.close();
        drop(relays);
        stop(handle, task).await;
    });
}
