use super::*;
use crate::workspace::{Update, WorkspaceUpdate};

#[tokio::test]
async fn terminal_delivery_remains_available_when_the_executor_bridge_is_full() {
    let (send, updates) = channel();
    let mut state = crate::workspace::state::WorkspaceState::default();
    let identity = crate::workspace::state::Identity {
        generation: Some(7),
        expires_at: 100,
        rejected: None,
    };
    let request = state.start(&identity).unwrap();
    for _ in 0..DELIVERY_CAPACITY {
        assert!(
            send.try_send(WorkspaceUpdate(Update::Channels(
                request.clone(),
                Err(crate::api::ApiError::Unavailable),
            )))
            .is_ok()
        );
    }
    assert!(matches!(
        send.try_send(WorkspaceUpdate(Update::Tick)),
        Err(async_channel::TrySendError::Full(_))
    ));
    assert!(
        send.send_terminal(WorkspaceUpdate(Update::Tick))
            .await
            .is_ok()
    );
    assert_eq!(updates.len(), DELIVERY_CAPACITY + 1);
    assert!(matches!(updates.recv().await.unwrap().0, Update::Tick));
    for _ in 0..DELIVERY_CAPACITY {
        assert!(matches!(
            updates.try_recv().unwrap().0,
            Update::Channels(..)
        ));
    }
    assert!(updates.try_recv().is_err());
    assert!(send.send(WorkspaceUpdate(Update::Tick)).await.is_ok());
    send.close();
    assert!(matches!(updates.recv().await.unwrap().0, Update::Tick));
    assert!(updates.recv().await.is_err());
}
