use bevy::prelude::*;
use maud::html;
use bevy_html::{HTMLPlugin, HTMLScene, NamedSystemRegistryExt};

#[derive(Reflect, Component, Default)]
#[reflect(Component, Default)]
struct BackgroundColorOnInteract {
    default: Color,
    hovered: Color,
    pressed: Color
}
fn hover_background_color(mut to_set: Query<(&Interaction, &BackgroundColorOnInteract, &mut BackgroundColor)>) {
    for (interact, colors, mut background) in &mut to_set {
        match interact {
            Interaction::None => {background.0 = colors.default;},
            Interaction::Pressed => {background.0 = colors.pressed;},
            Interaction::Hovered => {background.0 = colors.hovered;},
        }
    }
}

#[derive(Resource, Default)]
struct Number(i32);

fn startup(num: Res<Number>, mut html_assets: ResMut<Assets<HTMLScene>>, mut commands: Commands) {
    // UI camera
    commands.spawn(Camera2dBundle::default());

    let xs = HTMLScene::from(html! {
        Node Style="width: Percent(100), height: Percent(100), justify_content: Center, align_items: Center" {
            Node
            Style="flex_direction: Row,
                column_gap: Px(10),
                align_items: Center,
                margin: All(Px(20)),
                padding: All(Px(10))"
            Outline="color: \"black\", width: Px(1)"
            BackgroundColor="\"#111\""
            {
                // Node Handle:HTMLScene="\"include.html\"" { }
                // Node Handle:HTMLScene="\"embedded://bevy_html/widgets/Node.html\"" { }

                UiImage x="texture: \"cool.png\"" Style="width: Px(50), height: Px(50)" { }

                Node Style="flex_direction: Column, row_gap: Px(10)" {
                    Button BackgroundColorOnInteract="default: \"#966\", hovered: \"#A77\", pressed: \"#855\"" Style="padding: All(Px(10))"
                    XTarget="Name(\"number\")" XFunction="\"increment\"" XOn="Click" {
                        Text TextStyle="size: 30" { "increment" }
                    }
                    Button BackgroundColorOnInteract="default: \"#669\", hovered: \"#77A\", pressed: \"#558\"" Style="padding: All(Px(10))"
                    XTarget="Name(\"number\")" XFunction="\"decrement\"" XOn="Click" {
                        Text TextStyle="size: 30" { "decrement" }
                    }
                }
    
                (number(num))
            }
        }
    });

    commands.spawn_empty()
        .insert(html_assets.add(xs));
}

fn number(num: Res<Number>) -> HTMLScene {
    HTMLScene::try_from(format!(r##"
        <Node id="number" Style="width: Px(50), height: Px(50), justify_content: Center, align_items: Center" BackgroundColor='"white"'>
            <Text TextStyle='size: 30, color: "#222", font: "FreeSerif.ttf"'>{}</Text>
        </Node>
    "##, num.0)).unwrap()
}
fn increment(mut num: ResMut<Number>) -> HTMLScene {
    num.0 += 1;
    number(num.into())
}
fn decrement(mut num: ResMut<Number>) -> HTMLScene {
    num.0 -= 1;
    number(num.into())
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(HTMLPlugin)

        .init_resource::<Number>()
        .register_named_system("increment", increment)
        .register_named_system("decrement", decrement)

        .add_systems(Startup, startup)
        .register_type::<BackgroundColorOnInteract>()
        .add_systems(Update, hover_background_color)

        .run();
}