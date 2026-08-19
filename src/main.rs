use iced::widget::{container, text};
use iced::window;
use iced::{Color, Element, Length, Task};
use iced_layershell::build_pattern::daemon;
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;

const HEIGHT: u32 = 32;

fn main() -> Result<(), iced_layershell::Error> {
    daemon(Bar::default, namespace, update, view)
        .style(style)
        .settings(Settings {
            layer_settings: LayerShellSettings {
                anchor: Anchor::Top | Anchor::Left | Anchor::Right,
                size: Some((0, HEIGHT)),
                // Defaults to -1, which lets tiled windows render underneath us.
                exclusive_zone: HEIGHT as i32,
                layer: Layer::Top,
                // Defaults to OnDemand, which would let the bar steal keyboard focus.
                keyboard_interactivity: KeyboardInteractivity::None,
                // Requires `daemon`; `application` asserts against this mode.
                start_mode: StartMode::AllScreens,
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
}

#[derive(Default)]
struct Bar;

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
enum Message {}

fn namespace() -> String {
    String::from("ricebar")
}

fn update(_bar: &mut Bar, _message: Message) -> Task<Message> {
    Task::none()
}

fn view(_bar: &Bar, _id: window::Id) -> Element<'_, Message> {
    container(text("ricebar"))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_y(Length::Fill)
        .padding([0, 12])
        .into()
}

fn style(_bar: &Bar, _theme: &iced::Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: Color::from_rgb8(0x1e, 0x1e, 0x2e),
        text_color: Color::from_rgb8(0xcd, 0xd6, 0xf4),
    }
}
