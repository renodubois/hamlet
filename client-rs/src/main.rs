use iced::{Element, Task};

use crate::views::{
    app::App,
    login_form::{LoginForm, Message::SuccesfulLogin},
};

mod api;
mod views;

fn main() -> iced::Result {
    iced::application(Hamlet::new, Hamlet::update, Hamlet::view)
        .theme(Hamlet::theme)
        .run()
}

struct Hamlet {
    login: LoginForm,
    app: Option<App>,
}

struct Channel {
    name: String,
    id: String,
}

#[derive(Debug, Clone)]
enum Message {
    App(views::app::Message),
    Login(views::login_form::Message),
}

impl Hamlet {
    fn new() -> Self {
        Hamlet {
            login: LoginForm::new(),
            app: None,
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Login(message) => match message {
                SuccesfulLogin(res) => {
                    let (app, task) = App::new(res.user, res.server_url, res.cookies);
                    self.app = Some(app);

                    task.map(Message::App)
                }
                _ => self.login.update(message).map(Message::Login),
            },
            Message::App(msg) => self
                .app
                .as_mut()
                .expect("App should be initialized if user is logged in")
                .update(msg)
                .map(Message::App),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        match &self.app {
            Some(app) => app.view().map(Message::App),
            None => self.login.view().map(Message::Login),
        }
    }

    fn theme(&self) -> iced::Theme {
        iced::Theme::Dracula
    }
}
