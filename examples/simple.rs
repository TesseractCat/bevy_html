use bevy::{prelude::*, render::render_resource::DynamicUniformBuffer, reflect::{TypeRegistry, TypeRegistration}, scene::DynamicEntity, app::ScheduleRunnerPlugin, pbr::PBR_TYPES_SHADER_HANDLE, ecs::{reflect::ReflectCommandExt, system::{EntityCommands, SystemId}}, ui::FocusPolicy, a11y::Focus};
use maud::{html, Markup};
use spring_peeper::{HTMLPlugin, HTMLScene, NamedSystemRegistryExt};

fn startup(mut html_assets: ResMut<Assets<HTMLScene>>, mut commands: Commands) {
    // UI camera
    commands.spawn(Camera2dBundle::default());

    let xs = HTMLScene::from(html! {
        NodeTemplate
        Style="{flex_direction: Row, row_gap: Px(10), margin: {left: Px(20), right: Px(20), top: Px(20), bottom: Px(20)}}"
        BackgroundColor="Rgba(red: 0, green: 0, blue: 0, alpha: 0)"
        //Handle:XScene="test.xml"
        {
            NodeTemplate Style="{flex_direction: Column, row_gap: Px(10)}" BackgroundColor="Rgba(red: 0, green: 0, blue: 0, alpha: 0)" {
                NodeTemplate Style="{width: Px(50)}"
                    ContentSize UiImage="{texture: (path_to_handle, (\"Image\", \"cool.png\"))}" UiImageSize { }
                TextTemplate BackgroundColor="(hex_to_color, \"FF0000\")"
                    Button Interaction="None" XSwap XFunction="\"foo\"" { "meowing" }
                TextTemplate BackgroundColor="Rgba(red: 0, green: 1, blue: 0, alpha: 1)" { "barking" }
                TextTemplate BackgroundColor="Rgba(red: 0, green: 0, blue: 1, alpha: 1)" { "shouting" }
            }
            NodeTemplate #reds Style="{flex_direction: Column, row_gap: Px(5)}" {
                @for i in 0..10 {
                    TextTemplate BackgroundColor={"Rgba(red: " (format!("{}", (i as f32)/10.)) ", green: 0, blue: 0, alpha: 1)"} { "red" }
                }
            }
        }
    });
    let xs2 = HTMLScene::try_from(r##"
    <TextTemplate Style='{ flex_direction: Column }' BackgroundColor='(hex_to_color, "#FF0000")'>Eating</TextTemplate>
    "##).unwrap();

    commands.spawn_empty()
        .insert(html_assets.add(xs));
}

fn foo() -> HTMLScene {
    HTMLScene::from(html! {
        NodeTemplate Style="{flex_direction: Column, row_gap: Px(20)}" {
            TextTemplate BackgroundColor="Rgba(red: 0, green: 0, blue: 1, alpha: 1)" TextStyle="(30, Rgba(red:0,green:0,blue:0,alpha:1))" { "cheese" }
            TextTemplate BackgroundColor="Rgba(red: 0, green: 0, blue: 1, alpha: 1)" { "cheese" }
            TextTemplate BackgroundColor="Rgba(red: 0, green: 0, blue: 1, alpha: 1)" { "cheese" }
            TextTemplate BackgroundColor="Rgba(red: 0, green: 0, blue: 1, alpha: 1)" { "cheese" }

            NodeTemplate Style="{width: Px(60)}"
                ContentSize UiImage="{texture: (path_to_handle, (\"Image\", \"cool.png\"))}" UiImageSize { }
            NodeTemplate Style="{width: Px(70)}"
                ContentSize UiImage="{texture: (path_to_handle, (\"Image\", \"cool.png\"))}" UiImageSize { }
        }
    })
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(HTMLPlugin)
        .register_named_system("foo", foo)

        .add_systems(Startup, startup)

        .run();
}