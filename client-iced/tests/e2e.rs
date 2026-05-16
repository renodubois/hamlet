#![allow(clippy::expect_used)]

use hamlet_client_iced::api::ApiError;
use hamlet_client_iced::app::{AppMessage, AppState};
use hamlet_client_iced::auth::ChannelListState;
use hamlet_client_iced::test_support::fake_api::FakeApi;
use hamlet_client_iced::test_support::fake_storage::FakeStorage;
use hamlet_client_iced::test_support::harness::ReducerHarness;
use iced_test::selector::{Candidate, Target};
use iced_test::{Selector, Simulator, selector};

use hamlet_client_iced::app::widget_ids::{
    COMPOSER_INPUT_ID, CREATE_CHANNEL_NAME_INPUT_ID, EMOJI_SEARCH_INPUT_ID, PASSWORD_INPUT_ID,
    USERNAME_INPUT_ID,
};

// Iced's first-party test crate supports headless UI tests through Simulator
// and Emulator. These smoke tests use Simulator with the native client's
// deterministic reducer fakes, so they exercise the rendered Iced widgets and
// user-facing flows without relying on a desktop display server.
const VIEWPORT: iced::Size = iced::Size::new(1200.0, 800.0);

type UiResult = Result<(), iced_test::Error>;

#[test]
fn rejects_incorrect_credentials_and_stays_on_the_login_screen() -> UiResult {
    let api = FakeApi::default();
    api.set_next_login_result(Err(ApiError::InvalidCredentials))
        .expect("configure fake login failure");
    let mut harness = boot_ready_with_api(api);

    submit_login(&mut harness, "baipas", "definitely-wrong")?;

    assert_visible(&harness.state, "Invalid username or password.")?;
    assert_visible(&harness.state, "Username")
}

#[test]
fn logs_in_as_the_dev_user_and_lands_in_general() -> UiResult {
    let mut harness = boot_ready();

    login_as_dev_user(&mut harness)?;

    assert_visible(&harness.state, "Signed in as baipas")?;
    assert_visible(&harness.state, "# general")?;
    assert_visible(&harness.state, "Baipas: hello")
}

#[test]
fn sends_a_message_and_sees_it_render_in_the_channel() -> UiResult {
    let mut harness = boot_ready();
    login_as_dev_user(&mut harness)?;

    let marker = format!("hello from iced e2e {}", unique_suffix());
    drive(&mut harness, |ui| {
        type_into(ui, COMPOSER_INPUT_ID, &marker)?;
        ui.click("Send")?;
        Ok(())
    })?;

    assert_visible(&harness.state, &format!("baipas: {marker}"))
}

#[test]
fn selects_an_emoji_from_the_picker_and_sends_it() -> UiResult {
    let mut harness = boot_ready();
    login_as_dev_user(&mut harness)?;

    let marker = format!("emoji from iced e2e {} ", unique_suffix());
    let expected = format!("{marker}❤️");

    drive(&mut harness, |ui| {
        type_into(ui, COMPOSER_INPUT_ID, &marker)?;
        ui.click("Emoji")?;
        Ok(())
    })?;
    assert_visible(&harness.state, "Emoji picker")?;

    drive(&mut harness, |ui| {
        type_into(ui, EMOJI_SEARCH_INPUT_ID, "heart")?;
        Ok(())
    })?;

    drive(&mut harness, |ui| {
        ui.click(text_containing("red heart"))?;
        ui.click("Send")?;
        Ok(())
    })?;

    assert_visible(&harness.state, &format!("baipas: {expected}"))
}

#[test]
fn rearranges_channels_with_move_buttons_and_persists_the_new_order() -> UiResult {
    let api = FakeApi::default();
    let storage = FakeStorage::default();
    let mut harness = boot_ready_with_storage_and_api(storage.clone(), api.clone());
    login_as_dev_user(&mut harness)?;

    let suffix = unique_suffix();
    let alpha = format!("alpha-{suffix}");
    let bravo = format!("bravo-{suffix}");
    create_channel(&mut harness, &alpha)?;
    create_channel(&mut harness, &bravo)?;

    let before = loaded_channel_names(&harness);
    let alpha_before = before
        .iter()
        .position(|name| name == &alpha)
        .expect("alpha channel should exist after creation");
    let bravo_before = before
        .iter()
        .position(|name| name == &bravo)
        .expect("bravo channel should exist after creation");
    assert!(bravo_before > alpha_before);

    // Native Iced uses move buttons instead of the web client's drag-and-drop
    // row behavior. The seeded order is general, voice, alpha, bravo, so three
    // upward moves put bravo above general.
    for _ in 0..3 {
        let bravo_position = loaded_channel_names(&harness)
            .iter()
            .position(|name| name == &bravo)
            .expect("bravo channel should exist before moving");
        drive(&mut harness, |ui| {
            ui.click(text_exact_occurrence("Move up", bravo_position))?;
            Ok(())
        })?;
    }

    let after = loaded_channel_names(&harness);
    assert_eq!(after.first(), Some(&bravo));
    let bravo_after = after
        .iter()
        .position(|name| name == &bravo)
        .expect("bravo channel should still exist after reordering");
    let general_after = after
        .iter()
        .position(|name| name == "general")
        .expect("general channel should still exist after reordering");
    let alpha_after = after
        .iter()
        .position(|name| name == &alpha)
        .expect("alpha channel should still exist after reordering");
    assert!(general_after > bravo_after);
    assert!(alpha_after > general_after);

    let restarted = boot_ready_with_storage_and_api(storage, api);
    assert_visible(&restarted.state, "Signed in as baipas")?;
    let restarted_order = loaded_channel_names(&restarted);
    assert_eq!(restarted_order.first(), Some(&bravo));

    Ok(())
}

fn boot_ready() -> ReducerHarness {
    boot_ready_with_api(FakeApi::default())
}

fn boot_ready_with_api(api: FakeApi) -> ReducerHarness {
    boot_ready_with_storage_and_api(FakeStorage::default(), api)
}

fn boot_ready_with_storage_and_api(storage: FakeStorage, api: FakeApi) -> ReducerHarness {
    let mut harness = ReducerHarness::boot_with_storage_and_api(storage, api);
    harness.run_all_effects();
    harness
}

fn login_as_dev_user(harness: &mut ReducerHarness) -> UiResult {
    submit_login(harness, "baipas", "password")
}

fn submit_login(harness: &mut ReducerHarness, username: &str, password: &str) -> UiResult {
    drive(harness, |ui| {
        type_into(ui, USERNAME_INPUT_ID, username)?;
        type_into(ui, PASSWORD_INPUT_ID, password)?;
        ui.click("Log in")?;
        Ok(())
    })
}

fn create_channel(harness: &mut ReducerHarness, name: &str) -> UiResult {
    drive(harness, |ui| {
        type_into(ui, CREATE_CHANNEL_NAME_INPUT_ID, name)?;
        ui.click("Create")?;
        Ok(())
    })?;

    assert_visible(&harness.state, &format!("# {name}"))
}

fn drive(
    harness: &mut ReducerHarness,
    action: impl for<'a> FnOnce(&mut Simulator<'a, AppMessage>) -> UiResult,
) -> UiResult {
    let messages = {
        let mut ui = render(&harness.state);
        action(&mut ui)?;
        ui.into_messages().collect::<Vec<_>>()
    };

    for message in messages {
        harness.dispatch(message);
    }
    harness.run_all_effects();

    Ok(())
}

fn render(state: &AppState) -> Simulator<'_, AppMessage> {
    Simulator::with_size(
        iced::Settings::default(),
        VIEWPORT,
        hamlet_client_iced::app::view::view(state),
    )
}

fn type_into(ui: &mut Simulator<'_, AppMessage>, id: &'static str, text: &str) -> UiResult {
    ui.click(selector::id(id))?;
    ui.typewrite(text);
    Ok(())
}

fn assert_visible(state: &AppState, label: &str) -> UiResult {
    let mut ui = render(state);
    ui.find(label).map(|_| ())
}

fn text_containing(needle: &str) -> impl Selector<Output = Target> + '_ {
    move |candidate: Candidate<'_>| match &candidate {
        Candidate::Text { content, .. } if content.contains(needle) => {
            Some(Target::from(candidate))
        }
        _ => None,
    }
}

fn text_exact_occurrence(label: &str, target_index: usize) -> impl Selector<Output = Target> + '_ {
    let mut seen = 0;

    move |candidate: Candidate<'_>| match &candidate {
        Candidate::Text { content, .. } if *content == label => {
            if seen == target_index {
                Some(Target::from(candidate))
            } else {
                seen += 1;
                None
            }
        }
        _ => None,
    }
}

fn loaded_channel_names(harness: &ReducerHarness) -> Vec<String> {
    let signed_in = harness
        .state
        .signed_in
        .as_ref()
        .expect("e2e harness should be signed in");

    match &signed_in.channels {
        ChannelListState::Loaded(channels) => channels
            .iter()
            .map(|channel| channel.name.clone())
            .collect(),
        other => panic!("channels should be loaded, got {other:?}"),
    }
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after Unix epoch")
        .as_nanos()
}
