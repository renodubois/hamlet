use super::*;
use crate::conversation::{ConversationUpdate, Update};

#[tokio::test]
async fn terminal_delivery_remains_available_when_the_executor_bridge_is_full() {
    let (send, updates) = channel();
    let mut state = crate::conversation::state::Conversation::default();
    let identity = crate::conversation::state::Identity {
        generation: Some(7),
        expires_at: 100,
        rejected: None,
    };
    let request = state.start(&identity).unwrap();
    for _ in 0..DELIVERY_CAPACITY {
        assert!(
            send.try_send(ConversationUpdate(Update::Channels(
                request.clone(),
                Err(crate::api::ApiError::Unavailable),
            )))
            .is_ok()
        );
    }
    assert!(matches!(
        send.try_send(ConversationUpdate(Update::Tick)),
        Err(async_channel::TrySendError::Full(_))
    ));
    assert!(
        send.send_terminal(ConversationUpdate(Update::Tick))
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
    assert!(send.send(ConversationUpdate(Update::Tick)).await.is_ok());
    send.close();
    assert!(matches!(updates.recv().await.unwrap().0, Update::Tick));
    assert!(updates.recv().await.is_err());
}
