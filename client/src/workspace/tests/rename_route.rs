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
        settle(cx, &[&activity], || ids(&activity, &channel.id).contains(&message.id)).await;
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
        assert_eq!(activity.read().selected_channel().unwrap().name, "GENERAL");
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
