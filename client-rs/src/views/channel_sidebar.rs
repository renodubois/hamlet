use std::sync::Arc;

use iced::{
    Element, Task,
    widget::{button, column, row, text},
};

use crate::api::{ApiClient, ChannelResponse};

#[derive(Debug, Clone)]
pub enum Message {
    ChannelsLoaded(Vec<ChannelResponse>),
    ChannelsError,
    ActiveChannel(i64),
}

pub struct ChannelSidebar {
    channels: Vec<ChannelResponse>,
    active: i64,
    req_state: RequestState,
}

enum RequestState {
    Loading,
    Failed,
    Loaded,
}

impl ChannelSidebar {
    pub fn new(api: Arc<ApiClient>) -> (Self, Task<Message>) {
        (
            ChannelSidebar {
                channels: Vec::new(),
                // TODO(reno): this isn't a good defualt, we should make a channel active when initilizing
                // or use an option
                active: 0,
                req_state: RequestState::Loading,
            },
            Task::perform(
                async move { api.get_channels().await },
                move |ret| match ret {
                    Ok(chans) => Message::ChannelsLoaded(chans),
                    Err(err) => {
                        dbg!(err);
                        Message::ChannelsError
                    }
                },
            ),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ChannelsLoaded(chans) => {
                self.channels = chans;
                self.req_state = RequestState::Loaded;
                Task::none()
            }
            Message::ChannelsError => {
                self.req_state = RequestState::Failed;
                // TODO(reno): Retry logic here?
                Task::none()
            }
            Message::ActiveChannel(id) => {
                self.active = id;
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        match self.req_state {
            RequestState::Loading => text("Loading...").into(),
            RequestState::Failed => text("Error fetching channels").style(text::danger).into(),
            RequestState::Loaded => column(self.channels.iter().map(|channel| {
                let button_label = text(if self.active == channel.id {
                    channel.name.clone() + " (active)"
                } else {
                    channel.name.clone()
                });
                button(button_label)
                    .on_press(Message::ActiveChannel(channel.id.clone()))
                    .into()
            }))
            .padding(20)
            .spacing(10)
            .into(),
        }
    }
}
