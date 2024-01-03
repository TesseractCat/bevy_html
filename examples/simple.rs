use bevy::prelude::*;
use maud::html;
use bevy_html::{HTMLPlugin, HTMLScene, NamedSystemRegistryExt};

#[derive(Resource, Default)]
struct Number(i32);

fn startup(num: Res<Number>, mut html_assets: ResMut<Assets<HTMLScene>>, mut commands: Commands) {
    // UI camera
    commands.spawn(Camera2dBundle::default());

    let xs = HTMLScene::from(html! {
        NodeTemplate
        Style="flex_direction: Row,
            column_gap: Px(20),
            margin: (Px(20), Px(20), Px(20), Px(20)),
            padding: (Px(20), Px(20), Px(20), Px(20))"
        Outline="color: \"black\", width: Px(2)"
        BackgroundColor="Rgba(red: 1, green: 0, blue: 1, alpha: 1)"
        {
            NodeTemplate Style="width: Px(50), height: Px(50)" BackgroundColor="\"white\""
                ContentSize UiImage="texture: \"cool.png\"" UiImageSize Outline="width: Px(5), color: \"purple\"" { }

            (number(num))

            NodeTemplate Style="flex_direction: Column, row_gap: Px(10)" {
                NodeTemplate BackgroundColor="\"red\"" Style="padding: (Px(20),Px(20),Px(20),Px(20))"
                Button Interaction="None" XTarget="Name(\"number\")" XFunction="\"increment\"" {
                    TextTemplate TextStyle="size: 40" { "+" }
                }
                NodeTemplate BackgroundColor="\"blue\"" Style="padding: (Px(20),Px(20),Px(20),Px(20))"
                Button Interaction="None" XTarget="Name(\"number\")" XFunction="\"decrement\"" {
                    TextTemplate TextStyle="size: 40" { "-" }
                }
            }
        }
    });

    commands.spawn_empty()
        .insert(html_assets.add(xs));
}

fn number(num: Res<Number>) -> HTMLScene {
    HTMLScene::try_from(format!(r##"
        <NodeTemplate id="number" Style="padding: (Px(20), Px(20), Px(20), Px(20))" BackgroundColor='"white"'>
            <TextTemplate TextStyle='size: 35, color: "#222222", font: "FreeSerif.ttf"'>{}</TextTemplate>
        </NodeTemplate>
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

        .run();
}