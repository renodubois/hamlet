use iced::{
    Background, Border, Color, Element, Shadow, Task, color,
    widget::{button, column, text},
};

fn main() -> iced::Result {
    iced::application(Hamlet::new, Hamlet::update, Hamlet::view)
        .theme(Hamlet::theme)
        .run()
}

struct Hamlet {
    channels: Vec<Channel>,
    active_channel: String,
}

struct Channel {
    name: String,
    id: String,
}

#[derive(Debug, Clone)]
enum Message {
    ActiveChannel(String),
}

fn test_channels() -> Vec<Channel> {
    vec![
        Channel {
            name: "general".to_string(),
            id: "1".to_string(),
        },
        Channel {
            name: "cool_channel".to_string(),
            id: "2".to_string(),
        },
    ]
}

impl Hamlet {
    fn new() -> Self {
        Hamlet {
            channels: test_channels(),
            active_channel: String::new(),
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ActiveChannel(channel_id) => {
                self.active_channel = channel_id.clone();
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let channel_buttons = column(self.channels.iter().map(|channel| {
            let button_label = text(if self.active_channel == channel.id {
                channel.name.clone() + " (active)"
            } else {
                channel.name.clone()
            });
            button(button_label)
                .on_press(Message::ActiveChannel(channel.id.clone()))
                .into()
        }));

        channel_buttons.padding(20).spacing(10).into()
    }

    fn theme(&self) -> iced::Theme {
        iced::Theme::Dracula
    }
}
