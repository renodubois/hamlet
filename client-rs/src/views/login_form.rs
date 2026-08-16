use crate::api::{ApiClient, LoginResult};
use iced::{
    Element, Task,
    widget::{button, column, row, text, text_input},
};
use reqwest::header::HeaderMap;

#[derive(Debug, Clone)]
pub enum Message {
    UpdateUsername(String),
    UpdatePassword(String),
    Submit,
    SuccesfulLogin(LoginResult),
    UnsuccessfulLogin,
}

#[derive(Debug, Clone)]
pub struct LoginForm {
    server_url: String,
    username: String,
    password: String,
    server_url_error: Option<String>,
    username_error: Option<String>,
    password_error: Option<String>,
}

impl LoginForm {
    pub fn new() -> Self {
        LoginForm {
            server_url: "http://localhost:3030".to_string(),
            username: "baipas".to_string(),
            password: "password".to_string(),
            server_url_error: None,
            username_error: None,
            password_error: None,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::UpdateUsername(value) => self.username = value,
            Message::UpdatePassword(value) => self.password = value,
            Message::Submit => {
                self.server_url_error = None;
                self.username_error = None;
                self.password_error = None;

                if self.server_url == "" {
                    self.server_url_error = Some("Server URL cannot be blank".to_string());
                }
                if self.username == "" {
                    self.username_error = Some("Username cannot be blank".to_string());
                }
                if self.password == "" {
                    self.password_error = Some("Password cannot be blank".to_string());
                }

                if self.server_url_error.is_some()
                    || self.username_error.is_some()
                    || self.password_error.is_some()
                {
                    return Task::none();
                }

                return Task::perform(
                    ApiClient::login(
                        self.server_url.clone(),
                        self.username.clone(),
                        self.password.clone(),
                    ),
                    move |ret| match ret {
                        Ok(res) => Message::SuccesfulLogin(res),
                        Err(_) => Message::UnsuccessfulLogin,
                    },
                );
            }
            Message::SuccesfulLogin(res) => {
                println!("we did it reddit");
                dbg!(res);
            }
            Message::UnsuccessfulLogin => {
                // TODO(reno): Display error here, maybe reset password?
                return Task::none();
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        column![
            row![
                text("Server URL"),
                text(match &self.server_url_error {
                    Some(msg) => msg.clone(),
                    None => "".to_string(),
                })
                .style(text::danger)
            ]
            .spacing(10),
            text_input("Server URL", &self.server_url).on_input(|s| Message::UpdateUsername(s)),
            row![
                text("Username"),
                text(match &self.username_error {
                    Some(msg) => msg.clone(),
                    None => "".to_string(),
                })
                .style(text::danger)
            ]
            .spacing(10),
            text_input("Username", &self.username).on_input(|s| Message::UpdateUsername(s)),
            row![
                text("Password"),
                text(match &self.password_error {
                    Some(msg) => msg.clone(),
                    None => "".to_string(),
                })
                .style(text::danger)
            ]
            .spacing(10),
            text_input("Password", &self.password).on_input(|s| Message::UpdatePassword(s)),
            button(text("Submit")).on_press(Message::Submit)
        ]
        .into()
    }
}
