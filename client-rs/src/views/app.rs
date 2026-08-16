use std::sync::Arc;

use iced::{
    Element, Task,
    widget::{row, text},
};
use reqwest::header::HeaderMap;

use crate::{
    api::{ApiClient, User},
    views::{app::Message::Channels, channel_sidebar::ChannelSidebar},
};

pub struct App {
    user: User,
    api: Arc<ApiClient>,
    channels: ChannelSidebar,
}

#[derive(Debug, Clone)]
pub enum Message {
    Channels(crate::views::channel_sidebar::Message),
}

impl App {
    pub fn new(user: User, server_url: String, cookies: HeaderMap) -> (Self, Task<Message>) {
        let api = Arc::new(ApiClient {
            url: server_url,
            cookies,
        });
        let (channels, task) = ChannelSidebar::new(api.clone());
        (
            App {
                user: user,
                api,
                channels,
            },
            task.map(Message::Channels),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Channels(msg) => self.channels.update(msg),
        };
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        row![
            self.channels.view().map(Message::Channels),
            "Messages here, soon(tm)"
        ]
        .into()
    }
}
