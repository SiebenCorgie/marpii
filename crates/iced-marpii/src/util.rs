use iced::Rectangle;
use marpii::ash::vk;

///Gamma corrects a linear color, ignoring the alpha channel
pub fn gamma_correct(color: [f32; 4]) -> [f32; 4] {
    let (mut r, mut g, mut b) = (color[0], color[1], color[2]);
    r = r.powf(1.0 / 2.2);
    g = g.powf(1.0 / 2.2);
    b = b.powf(1.0 / 2.2);

    [r, g, b, color[3]]
}

///Produces the vulkan rectangle for a clip bound.
pub fn clip_to_rect2d(clip: Rectangle) -> vk::Rect2D {
    vk::Rect2D {
        offset: vk::Offset2D {
            x: clip.x.floor() as i32,
            y: clip.y.floor() as i32,
        },
        extent: vk::Extent2D {
            width: clip.width.ceil() as u32,
            height: clip.height.ceil() as u32,
        },
    }
}
